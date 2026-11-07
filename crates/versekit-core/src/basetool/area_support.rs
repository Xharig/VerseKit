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

//! What the three areas blueprints, stock and ships share
//! (`basetool_sync._pull_generic`, `_send`, `_send_unless_held`, `_store_ok`,
//! `_refused_ops`): fetching a feed with the state of the last time, sending
//! change sets behind the lock of the Basetool, and the check that an own file
//! can be read before anything is made of it.

use std::collections::BTreeMap;

use crate::basetool::connection::{get, get_str};
use crate::basetool::error::{ApiError, truthy};
use crate::basetool::feed::{self, Sent, list};
use crate::basetool::hold::{self, ChangeSet};
use crate::basetool::sync::Pass;
use crate::jsonio::{self, Map, Value, map_set};

/// The object under `key`, taken out of `map` (an empty one if there is none).
/// Put it back with [`put_object`]; whoever works on it holds it alone.
pub fn take_object(map: &mut Map, key: &str) -> Map {
    match map.iter().position(|(k, _)| k == key) {
        Some(at) => match map.remove(at).1 {
            Value::Object(inner) => inner,
            _ => Map::new(),
        },
        None => Map::new(),
    }
}

/// Puts an object back (at the end if it was taken out, as Python's
/// `setdefault` would have made it).
pub fn put_object(map: &mut Map, key: &str, inner: Map) {
    map_set(map, key, Value::Object(inner));
}

/// The text list under `key`, in order.
pub fn texts(map: &Map, key: &str) -> Vec<String> {
    match get(map, key) {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

/// A list of texts as a value.
pub fn text_array<'a>(items: impl IntoIterator<Item = &'a String>) -> Value {
    Value::Array(items.into_iter().map(Value::str).collect())
}

/// `{key: text}` of the strings under `key`.
pub fn text_map(map: &Map, key: &str) -> BTreeMap<String, String> {
    match get(map, key) {
        Some(Value::Object(inner)) => {
            inner.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()
        }
        _ => BTreeMap::new(),
    }
}

/// `int(value or 0)` of a count in an answer.
pub fn count_of(map: &Map, key: &str) -> i64 {
    match get(map, key) {
        Some(Value::Int(n)) => n.parse().unwrap_or(0),
        Some(Value::Float(x)) if x.is_finite() => *x as i64,
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

/// Whether an own file can be read: missing is fine (then empty is right), a
/// file with `format` 1 and the list under `list_key` too, anything else not.
///
/// ⚠⚠ The stores give an empty list for a damaged file. The sync would take
/// the stock or the hangar for empty, take everything over from the Basetool and
/// write the file over it: the own entries would be gone. So a file that is
/// there but cannot be read keeps the area untouched.
pub fn store_ok(path: &str, list_key: &str) -> bool {
    if !std::path::Path::new(path).exists() {
        return true;
    }
    match jsonio::read_json(path) {
        Ok(Value::Object(data)) => {
            matches!(get(&data, "format"), Some(Value::Int(n)) if n == "1")
                && matches!(get(&data, list_key), Some(Value::Array(_)))
        }
        _ => false,
    }
}

/// How a file looked when it was read (size and time of change), to see before
/// writing it back whether another place wrote it in between.
pub type Stamp = Option<(u64, std::time::SystemTime)>;

pub fn stamp_of(path: &str) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// The error of an area that found its own file changed or unreadable while the pass ran: the pass
/// ends without its state and the next one starts over.
pub fn store_busy(what: &str) -> ApiError {
    ApiError::internal("STORE_BUSY", what)
}

/// The instructions of an accepted send that the Basetool refused for
/// `refused_keys`: they stand in the next plan again.
pub fn refused_ops(
    change_set: &Value,
    keys: &[String],
    refused_keys: &std::collections::BTreeSet<String>,
) -> Vec<Value> {
    let ops: &[Value] = match change_set.get("ops") {
        Some(Value::Array(ops)) => ops,
        _ => &[],
    };
    ops.iter().zip(keys).filter(|(_, key)| refused_keys.contains(*key)).map(|(op, _)| op.clone()).collect()
}

/// The stand of the Basetool after fetching: `key_field` to the item, and
/// whether the cursor expired (then it is a new snapshot).
pub type Pulled = (BTreeMap<String, Value>, bool);

/// Fetch for stock and hangar. `sub` carries `cursor` and `server` (the stand
/// of the Basetool after the last fetching).
///
/// With a cursor it is a feed on top of `sub.server`; the deletion marks take
/// entries away. `CURSOR_EXPIRED` fetches the snapshot instead.
pub fn pull_generic(
    conn: &crate::basetool::connection::Connection,
    sub: &mut Map,
    route: &str,
    key_field: &str,
) -> Result<Pulled, ApiError> {
    let keyed = |items: &[&Value]| -> BTreeMap<String, Value> {
        items
            .iter()
            .filter_map(|i| i.get(key_field).and_then(Value::as_str).map(|k| (k.to_string(), (*i).clone())))
            .collect()
    };
    let cursor = get_str(sub, "cursor").filter(|c| !c.is_empty()).map(str::to_string);
    let set_cursor = |sub: &mut Map, cursor: Option<String>| {
        map_set(sub, "cursor", cursor.as_ref().map_or(Value::Null, Value::str));
    };
    let Some(cursor) = cursor else {
        let fetched = feed::fetch_all(conn, route, None)?;
        set_cursor(sub, fetched.cursor.clone());
        return Ok((keyed(&fetched.items()), false));
    };
    let mut known: BTreeMap<String, Value> = match get(sub, "server") {
        Some(Value::Object(inner)) => inner.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => BTreeMap::new(),
    };
    match feed::fetch_all(conn, route, Some(&cursor)) {
        Err(error) if error.code == "CURSOR_EXPIRED" => {
            let fetched = feed::fetch_all(conn, route, None)?;
            set_cursor(sub, fetched.cursor.clone());
            Ok((keyed(&fetched.items()), true))
        }
        Err(error) => Err(error),
        Ok(fetched) => {
            for page in &fetched.pages {
                for stone in list(page, "removed") {
                    if let Some(key) = stone.get("key").and_then(Value::as_str) {
                        known.remove(key);
                    }
                }
                for item in list(page, "items") {
                    if let Some(key) = item.get(key_field).and_then(Value::as_str) {
                        known.insert(key.to_string(), item.clone());
                    }
                }
            }
            set_cursor(sub, fetched.cursor.clone());
            Ok((known, false))
        }
    }
}

/// What came of sending a list of change sets.
#[derive(Clone, Debug, Default)]
pub struct Posted {
    /// `(key, result, reason)` of every instruction that was not applied.
    pub failed: Vec<(String, String, String)>,
    pub applied: i64,
    /// The keys of all accepted sends.
    pub posted: Vec<String>,
    /// The refused instructions of the accepted sends.
    pub refused: Vec<Value>,
    /// A send is held until the member confirms it.
    pub paused: bool,
    pub offers_reduced: i64,
    pub offers_removed: i64,
    pub detached: i64,
}

/// Sends the change sets of `route` behind the lock of the Basetool
/// (`_send_unless_held`): nothing goes out while the route is held; if the
/// Basetool holds a send, the loop ends there and the route is held.
pub fn send_unless_held(pass: &mut Pass<'_>, route: &str, sets: &[ChangeSet]) -> Result<Posted, ApiError> {
    let mut out = Posted::default();
    if !hold::may_send(pass.state, pass.confirmations, route, sets, pass.now, pass.release_now) {
        out.paused = true;
        return Ok(out);
    }
    for (number, (change_set, keys)) in sets.iter().enumerate() {
        match feed::send_change_set(pass.conn, pass.conn.config(), route, change_set)? {
            Sent::Held { confirm_url, error: _ } => {
                hold::hold(pass.state, pass.confirmations, route, &sets[number..], confirm_url, &out.refused, pass.now);
                out.paused = true;
                break;
            }
            Sent::Guarded(_) => {
                // The write guard of a test: kept back like a held send, but not remembered as one.
                out.paused = true;
                break;
            }
            Sent::Answer(result) => {
                out.posted.extend(keys.iter().cloned());
                out.applied += count_of(&result, "applied");
                out.offers_reduced += count_of(&result, "offersReduced");
                out.offers_removed += count_of(&result, "offersRemoved");
                out.detached += count_of(&result, "detachedFromMissions");
                let mut refused_keys = std::collections::BTreeSet::new();
                for entry in list(&result, "results") {
                    let index = match entry.get("index") {
                        Some(Value::Int(n)) => n.parse::<usize>().ok().filter(|i| *i < keys.len()),
                        _ => None,
                    };
                    let kind = entry.get("result").and_then(Value::as_str);
                    if let Some(index) = index
                        && kind != Some("unchanged")
                    {
                        let reason = match entry.get("reason") {
                            Some(v) if truthy(v) => crate::basetool::error::py_str(v),
                            _ => String::new(),
                        };
                        out.failed.push((keys[index].clone(), kind.unwrap_or_default().to_string(), reason));
                        refused_keys.insert(keys[index].clone());
                    }
                }
                out.refused.extend(refused_ops(change_set, keys, &refused_keys));
            }
        }
    }
    Ok(out)
}
