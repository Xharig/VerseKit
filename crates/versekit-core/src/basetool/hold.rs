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

//! Change sets held back by the Basetool (`basetool_sync._hold`,
//! `may_send`, `held_routes`, `confirm_link`, `op_hash`, `fingerprint`).
//!
//! On `MASS_CHANGE_CONFIRMATION_REQUIRED` the Basetool keeps a change set
//! until the member confirms it in the browser. Every new change set
//! replaces the held one and makes the confirmation link invalid. So the
//! state keeps per route `held[route] = {until, skip, fingerprint}`:
//!
//! * Until `until` nothing goes out on this route; fetching goes on.
//! * After that the same change set (same fingerprint) does not go out again
//!   by itself; only when the plan changed or the player presses a sync
//!   button (`release_now`).
//! * Once nothing is left to send (confirmed, or taken back here), the entry
//!   goes.
//!
//! The confirmation address is a secret and lives in memory only, never in
//! the state. After a restart there is no button for a running hold.

use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use crate::basetool::config::Config;
use crate::basetool::error::ApiError;
use crate::basetool::jose::{ascii_only, sha256};
use crate::jsonio::{self, Map, Value, map_set};

/// How long a confirmation address is valid.
pub const CONFIRM_LIFETIME: f64 = 30.0 * 60.0;

/// One change set and the local keys of its operations, in order.
pub type ChangeSet = (Value, Vec<String>);

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// An operation without `opId` as a short fingerprint.
pub fn op_hash(op: &Value) -> String {
    let without = match op {
        Value::Object(map) => Value::Object(map.iter().filter(|(k, _)| k != "opId").cloned().collect()),
        other => other.clone(),
    };
    hex(&sha256(ascii_only(&jsonio::dumps(&without, None, true)).as_bytes()))
}

fn ops(change_set: &Value) -> &[Value] {
    match change_set.get("ops") {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

/// A fingerprint of the operations, without `opId` and without the split
/// into change sets: the same as long as the same would go out. Operations
/// whose `op_hash` is in `skip` do not count.
pub fn fingerprint(change_sets: &[ChangeSet], skip: &[String]) -> String {
    let hashes: Vec<String> = change_sets
        .iter()
        .flat_map(|(change_set, _)| ops(change_set).iter().map(op_hash))
        .filter(|h| !skip.contains(h))
        .collect();
    hex(&sha256(hashes.join("|").as_bytes()))
}

fn held(state: &Map) -> Option<&Map> {
    state.iter().find(|(k, _)| k == "held").and_then(|(_, v)| v.as_object())
}

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Float(x)) => *x,
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn texts(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// The confirmation pages per route, in memory only.
#[derive(Default)]
pub struct Confirmations {
    pages: Mutex<BTreeMap<String, (Option<String>, f64)>>,
}

impl Confirmations {
    /// A confirmation page for `route`, valid until `until`.
    pub fn set(&self, route: &str, url: Option<String>, until: f64) {
        if let Ok(mut pages) = self.pages.lock() {
            pages.insert(route.to_string(), (url, until));
        }
    }

    /// Whether something was removed.
    fn clear(&self, route: &str) -> bool {
        self.pages.lock().map(|mut p| p.remove(route).is_some()).unwrap_or(false)
    }

    pub fn clear_all(&self) {
        if let Ok(mut pages) = self.pages.lock() {
            pages.clear();
        }
    }

    /// The confirmation page for `route` while it is valid.
    pub fn link(&self, route: &str, now: f64) -> Option<String> {
        let pages = self.pages.lock().ok()?;
        let (url, until) = pages.get(route)?;
        url.clone().filter(|_| now < *until)
    }

    /// Routes with a confirmation entry, for the status.
    pub fn routes(&self) -> Vec<String> {
        self.pages.lock().map(|p| p.keys().cloned().collect()).unwrap_or_default()
    }
}

/// The confirmation page from the error answer: only an https address at
/// the Basetool, otherwise none (then no button appears).
pub fn confirm_address(config: &Config, error: &ApiError) -> Option<String> {
    let url = error.problem_text("confirmationUrl").unwrap_or_default();
    let safe = config.trusted_address(url);
    if !url.is_empty() && safe.is_empty() {
        // Only the host: the path of a confirmation address is a secret,
        // and the error log can end up in a public report.
        let host = crate::basetool::jose::UrlParts::split(url).ok().and_then(|p| p.hostname()).unwrap_or_default();
        let shown: String = host.chars().take(80).collect();
        crate::errors::record("basetool_sync.confirm_address", &format!("Adresse außerhalb des Basetools: {shown}"));
    }
    Some(safe).filter(|s| !s.is_empty())
}

/// Holds `route`. `change_sets` are those from the held one on; `refused`
/// the refused operations of the accepted ones before: together they are
/// what the next plan contains again.
pub fn hold(
    state: &mut Map,
    confirmations: &Confirmations,
    route: &str,
    change_sets: &[ChangeSet],
    confirm_url: Option<String>,
    refused: &[Value],
    now: f64,
) {
    let mut skip: Vec<String> = refused.iter().map(op_hash).collect();
    skip.sort();
    let entry = Value::Object(vec![
        ("until".into(), Value::Float(now + CONFIRM_LIFETIME)),
        ("skip".into(), Value::Array(skip.into_iter().map(Value::Str).collect())),
        ("fingerprint".into(), Value::str(fingerprint(change_sets, &[]))),
    ]);
    let mut routes = held(state).cloned().unwrap_or_default();
    map_set(&mut routes, route, entry);
    map_set(state, "held", Value::Object(routes));
    confirmations.set(route, confirm_url, now + CONFIRM_LIFETIME);
}

fn clear_hold(state: &mut Map, confirmations: &Confirmations, route: &str) {
    let Some(map) = state.iter_mut().find(|(k, _)| k == "held").and_then(|(_, v)| match v {
        Value::Object(map) => Some(map),
        _ => None,
    }) else {
        return;
    };
    let before = map.len();
    map.retain(|(k, _)| k != route);
    if map.len() != before {
        confirmations.clear(route);
    }
}

/// Whether sending on `route` is allowed now (see the module text).
/// `release_now`: routes a sync button released for this pass.
pub fn may_send(
    state: &mut Map,
    confirmations: &Confirmations,
    route: &str,
    change_sets: &[ChangeSet],
    now: f64,
    release_now: &HashSet<String>,
) -> bool {
    let Some(entry) = held(state).and_then(|h| h.iter().find(|(k, _)| k == route)).map(|(_, v)| v.clone()) else {
        return true;
    };
    if change_sets.is_empty() {
        clear_hold(state, confirmations, route);
        return true;
    }
    if now < number(entry.get("until")) {
        return false;
    }
    let same = entry.get("fingerprint").and_then(Value::as_str)
        == Some(fingerprint(change_sets, &texts(entry.get("skip"))).as_str());
    if release_now.contains(route) || !same {
        clear_hold(state, confirmations, route);
        return true;
    }
    false
}

/// What is held right now: route → `pending` or `expired`.
pub fn held_routes(state: &Map, now: f64) -> Vec<(String, &'static str)> {
    held(state)
        .map(|h| {
            h.iter()
                .map(|(route, entry)| {
                    (route.clone(), if now < number(entry.get("until")) { "pending" } else { "expired" })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(key: &str, id: &str) -> Value {
        Value::Object(vec![
            ("opId".into(), Value::str(id)),
            ("op".into(), Value::str("upsert")),
            ("key".into(), Value::str(key)),
            ("menge".into(), Value::Float(1.5)),
        ])
    }

    fn set(ops: Vec<Value>) -> ChangeSet {
        let keys = ops.iter().map(|o| o.get("key").and_then(Value::as_str).unwrap_or_default().to_string()).collect();
        (Value::Object(vec![("ops".into(), Value::Array(ops))]), keys)
    }

    #[test]
    fn op_hash_ignores_the_op_id() {
        assert_eq!(op_hash(&op("a", "1")), op_hash(&op("a", "2")));
        assert_ne!(op_hash(&op("a", "1")), op_hash(&op("b", "1")));
        // The same split differently is the same fingerprint.
        let whole = [set(vec![op("a", "1"), op("b", "2")])];
        let split = [set(vec![op("a", "3")]), set(vec![op("b", "4")])];
        assert_eq!(fingerprint(&whole, &[]), fingerprint(&split, &[]));
    }

    #[test]
    fn a_held_route_waits_and_is_not_resent() {
        let mut state = Map::new();
        let confirmations = Confirmations::default();
        let sets = [set(vec![op("a", "1")])];
        let none = HashSet::new();
        hold(&mut state, &confirmations, "/me/stock/changes", &sets, Some("https://x".into()), &[], 1000.0);
        assert!(!may_send(&mut state, &confirmations, "/me/stock/changes", &sets, 1500.0, &none));
        assert_eq!(confirmations.link("/me/stock/changes", 1500.0).as_deref(), Some("https://x"));
        assert_eq!(held_routes(&state, 1500.0), [("/me/stock/changes".to_string(), "pending")]);
        // Expired: the same set stays held; another plan or a button sends.
        assert!(!may_send(&mut state, &confirmations, "/me/stock/changes", &sets, 3000.0, &none));
        assert_eq!(confirmations.link("/me/stock/changes", 3000.0), None);
        let other = [set(vec![op("b", "1")])];
        assert!(may_send(&mut state, &confirmations, "/me/stock/changes", &other, 3000.0, &none));
        assert!(held_routes(&state, 3000.0).is_empty());
        hold(&mut state, &confirmations, "/r", &sets, None, &[], 1000.0);
        let released: HashSet<String> = ["/r".to_string()].into();
        assert!(may_send(&mut state, &confirmations, "/r", &sets, 3000.0, &released));
        hold(&mut state, &confirmations, "/r", &sets, None, &[], 1000.0);
        assert!(may_send(&mut state, &confirmations, "/r", &[], 1100.0, &none));
    }
}
