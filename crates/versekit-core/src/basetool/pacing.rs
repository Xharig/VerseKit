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

//! How sparingly the change sets go out. The Basetool keeps every answer to a
//! change set for 24 hours, and these answers fill a budget per member
//! (`EXCHANGE_BUDGET_EXHAUSTED`).
//!
//! * **An answered operation that was not applied** (refused, or `unchanged`)
//!   is remembered per route by its [`op_hash`]: as long as the plan holds the
//!   same operation, it does not go out again, and its refusal still counts for
//!   the area. A changed amount here or over there makes another operation, and
//!   that one goes out. After [`QUIET_FOR`] or a sync button it goes out once
//!   more.
//! * **`Retry-After` on a send** stops every send route until that time; the
//!   pass goes on with fetching. Both live in the state of the installation
//!   and so outlast a restart.

use std::collections::BTreeSet;

use crate::basetool::error::{Action, ApiError};
use crate::basetool::hold::{ChangeSet, op_hash};
use crate::jsonio::{Map, Value, map_set};

/// How long an answered operation that was not applied stays back.
pub const QUIET_FOR: f64 = 24.0 * 3600.0;
/// The state key of the remembered operations: route → hash → `{key, result, reason, at}`.
pub const QUIET: &str = "quiet";
/// The state key of the wait after `Retry-After`: `{until, code}`.
pub const WAIT: &str = "send_wait";

/// The refusal of an operation: local key, result, reason.
pub type Failure = (String, String, String);

/// What the Basetool answered to one sent operation: its hash, its local key, and `result` and `reason` when the
/// answer listed it (an operation the answer does not list was applied).
pub type Answered = (String, String, Option<(String, String)>);

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Float(x)) => *x,
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_string()
}

fn object<'a>(map: &'a Map, key: &str) -> Option<&'a Map> {
    map.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_object())
}

/// Until when no change set may go out, while that time lies ahead.
pub fn wait_until(state: &Map, now: f64) -> Option<f64> {
    let until = number(object(state, WAIT)?.iter().find(|(k, _)| k == "until").map(|(_, v)| v));
    (now < until).then_some(until)
}

/// After a refused send: with `Retry-After` and an error worth trying again, the wait goes into the state and
/// its end comes back. Anything else leaves the state as it is.
pub fn wait_after(state: &mut Map, now: f64, error: &ApiError) -> Option<f64> {
    let seconds = error.retry_after.filter(|s| *s > 0)?;
    if error.action() != Action::Retry {
        return None;
    }
    let until = now + seconds as f64;
    let until = wait_until(state, now).map_or(until, |known| known.max(until));
    let entry = Value::Object(vec![("until".into(), Value::Float(until)), ("code".into(), Value::str(&error.code))]);
    map_set(state, WAIT, entry);
    Some(until)
}

/// Takes a wait that is over out of the state.
pub fn clear_expired_wait(state: &mut Map, now: f64) {
    if object(state, WAIT).is_some() && wait_until(state, now).is_none() {
        state.retain(|(k, _)| k != WAIT);
    }
}

fn remembered(state: &Map, route: &str) -> Map {
    object(state, QUIET).and_then(|routes| object(routes, route)).cloned().unwrap_or_default()
}

fn ops(change_set: &Value) -> &[Value] {
    match change_set.get("ops") {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

/// The change sets without the operations that stay back, and the refusals remembered for these. `release`: a
/// sync button, everything goes.
pub fn leave_out(state: &Map, route: &str, sets: &[ChangeSet], now: f64, release: bool) -> (Vec<ChangeSet>, Vec<Failure>) {
    let known = if release { Map::new() } else { remembered(state, route) };
    let mut failed = Vec::new();
    let mut out = Vec::new();
    for (change_set, keys) in sets {
        let mut kept_ops = Vec::new();
        let mut kept_keys = Vec::new();
        for (op, key) in ops(change_set).iter().zip(keys) {
            let entry = known.iter().find(|(hash, _)| *hash == op_hash(op)).map(|(_, v)| v);
            match entry.filter(|e| now - number(e.get("at")) < QUIET_FOR) {
                Some(entry) => {
                    let result = text(entry.get("result"));
                    if result != "unchanged" {
                        failed.push((key.clone(), result, text(entry.get("reason"))));
                    }
                }
                None => {
                    kept_ops.push(op.clone());
                    kept_keys.push(key.clone());
                }
            }
        }
        if kept_ops.is_empty() {
            continue;
        }
        let mut body = match change_set {
            Value::Object(map) => map.clone(),
            _ => Map::new(),
        };
        map_set(&mut body, "ops", Value::Array(kept_ops));
        out.push((Value::Object(body), kept_keys));
    }
    (out, failed)
}

/// Remembers what was answered without being applied; what the plan no longer holds and what was applied now is
/// forgotten. `planned`: all change sets of the plan, before [`leave_out`].
pub fn remember(state: &mut Map, route: &str, planned: &[ChangeSet], answered: &[Answered], now: f64) {
    let hashes: BTreeSet<String> = planned.iter().flat_map(|(set, _)| ops(set).iter().map(op_hash)).collect();
    let mut kept: Map = remembered(state, route)
        .into_iter()
        .filter(|(hash, entry)| hashes.contains(hash) && now - number(entry.get("at")) < QUIET_FOR)
        .collect();
    for (hash, key, outcome) in answered {
        kept.retain(|(h, _)| h != hash);
        if let Some((result, reason)) = outcome.as_ref().filter(|(result, _)| result != "applied") {
            kept.push((
                hash.clone(),
                Value::Object(vec![
                    ("key".into(), Value::str(key)),
                    ("result".into(), Value::str(result)),
                    ("reason".into(), Value::str(reason)),
                    ("at".into(), Value::Float(now)),
                ]),
            ));
        }
    }
    let mut routes = object(state, QUIET).cloned().unwrap_or_default();
    if kept.is_empty() {
        routes.retain(|(k, _)| k != route);
    } else {
        map_set(&mut routes, route, Value::Object(kept));
    }
    if routes.is_empty() {
        state.retain(|(k, _)| k != QUIET);
    } else {
        map_set(state, QUIET, Value::Object(routes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(key: &str, amount: f64) -> Value {
        Value::Object(vec![
            ("opId".into(), Value::str("s1")),
            ("op".into(), Value::str("set-quantity")),
            ("key".into(), Value::str(key)),
            ("quantity".into(), Value::Float(amount)),
        ])
    }

    fn set(ops: Vec<(&str, f64)>) -> ChangeSet {
        let keys = ops.iter().map(|(k, _)| (*k).to_string()).collect();
        (Value::Object(vec![("ops".into(), Value::Array(ops.iter().map(|(k, a)| op(k, *a)).collect()))]), keys)
    }

    fn refused(hash_of: &Value, key: &str, reason: &str) -> Answered {
        (op_hash(hash_of), key.into(), Some(("rejected".into(), reason.into())))
    }

    #[test]
    fn a_refused_operation_stays_back_with_its_reason_until_it_changes_a_day_passes_or_a_button_releases_it() {
        let mut state = Map::new();
        let planned = [set(vec![("a", 1.0), ("b", 2.0)])];
        remember(&mut state, "/r", &planned, &[refused(&op("a", 1.0), "a", "AMBIGUOUS"), (op_hash(&op("b", 2.0)), "b".into(), None)], 100.0);
        let (sent, failed) = leave_out(&state, "/r", &planned, 200.0, false);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1, ["b"], "the applied one is not remembered");
        assert_eq!(failed, [("a".to_string(), "rejected".to_string(), "AMBIGUOUS".to_string())]);
        // Only "a" planned: nothing goes out.
        let only_a = [set(vec![("a", 1.0)])];
        assert!(leave_out(&state, "/r", &only_a, 200.0, false).0.is_empty());
        // Another amount is another operation.
        let changed = [set(vec![("a", 3.0)])];
        assert_eq!(leave_out(&state, "/r", &changed, 200.0, false).0.len(), 1);
        // A button, or a day later.
        assert_eq!(leave_out(&state, "/r", &only_a, 200.0, true).0.len(), 1);
        assert_eq!(leave_out(&state, "/r", &only_a, 100.0 + QUIET_FOR, false).0.len(), 1);
        // No longer planned: forgotten, and the key goes once nothing is left.
        remember(&mut state, "/r", &[], &[], 300.0);
        assert!(state.is_empty(), "{state:?}");
    }

    #[test]
    fn an_unchanged_answer_stays_back_without_counting_as_a_refusal() {
        let mut state = Map::new();
        let planned = [set(vec![("a", 1.0)])];
        let answer = (op_hash(&op("a", 1.0)), "a".to_string(), Some(("unchanged".to_string(), String::new())));
        remember(&mut state, "/r", &planned, &[answer], 0.0);
        let (sent, failed) = leave_out(&state, "/r", &planned, 1.0, false);
        assert!(sent.is_empty() && failed.is_empty());
    }

    #[test]
    fn retry_after_stops_sending_until_its_time_and_a_longer_wait_wins() {
        let mut state = Map::new();
        let budget = ApiError::new("EXCHANGE_BUDGET_EXHAUSTED", 503).with_retry_after(Some(127));
        assert_eq!(wait_after(&mut state, 1000.0, &budget), Some(1127.0));
        assert_eq!(wait_until(&state, 1126.9), Some(1127.0));
        assert_eq!(wait_until(&state, 1127.0), None, "from its time on it sends again");
        let shorter = ApiError::new("RATE_LIMITED", 429).with_retry_after(Some(10));
        assert_eq!(wait_after(&mut state, 1100.0, &shorter), Some(1127.0));
        // Without Retry-After, or with an error that a later attempt does not mend, nothing waits.
        let mut empty = Map::new();
        assert_eq!(wait_after(&mut empty, 0.0, &ApiError::new("SERVICE_UNAVAILABLE", 503)), None);
        assert_eq!(wait_after(&mut empty, 0.0, &ApiError::new("FORBIDDEN", 403).with_retry_after(Some(5))), None);
        assert!(empty.is_empty());
        clear_expired_wait(&mut state, 1200.0);
        assert!(state.is_empty());
    }
}
