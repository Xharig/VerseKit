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

//! The requests every area shares (`basetool_sync._fetch_all`,
//! `_resolve_refs`, `_locations`, `exchange_orgstock.send`): snapshot and
//! feed to the end, resolving names to Basetool ids, the storage locations,
//! sending one change set.
//!
//! ⚠ Ids (`key`, `id`, `ref`, `bt`) are opaque: stored and sent back, never
//! taken apart.

use crate::basetool::config::Config;
use crate::basetool::connection::{Connection, get, get_str, new_idempotency_key};
use crate::basetool::error::{ApiError, truthy};
use crate::basetool::hold;
use crate::jsonio::{Map, Value, map_set};

pub const PAGE_LIMIT: usize = 500;
/// Most references per `catalog/resolve`.
pub const RESOLVE_MAX: usize = 500;
/// Something not resolvable is asked again at most once a day.
pub const RESOLVE_AGAIN: f64 = 24.0 * 3600.0;
/// The storage locations change seldom; once an hour is enough.
pub const LOCATIONS_AGAIN: f64 = 3600.0;
pub const MASS_CHANGE: &str = "MASS_CHANGE_CONFIRMATION_REQUIRED";

/// Pages of a snapshot or feed and the cursor after the last one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fetched {
    pub pages: Vec<Map>,
    pub cursor: Option<String>,
}

impl Fetched {
    /// The items of all pages in order.
    pub fn items(&self) -> Vec<&Value> {
        self.pages.iter().flat_map(|p| list(p, "items")).collect()
    }

    /// The keys of the deletion marks of all pages in order.
    pub fn removed_keys(&self) -> Vec<String> {
        self.pages
            .iter()
            .flat_map(|p| list(p, "removed"))
            .filter_map(|stone| stone.get("key").and_then(Value::as_str))
            .map(str::to_string)
            .collect()
    }
}

/// A list value of an answer; anything else is empty.
pub fn list<'a>(map: &'a Map, key: &str) -> &'a [Value] {
    match get(map, key) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

/// The feed from `cursor` on, or the snapshot without one, to the end.
///
/// A page that says `hasMore` but brings no new cursor would be asked
/// for again and again; that ends with `FEED_STUCK`.
pub fn fetch_all(conn: &Connection, route: &str, cursor: Option<&str>) -> Result<Fetched, ApiError> {
    let limit = PAGE_LIMIT.to_string();
    let mut cursor = cursor.filter(|c| !c.is_empty()).map(str::to_string);
    let mut pages = Vec::new();
    loop {
        let mut query: Vec<(&str, &str)> = vec![("limit", &limit)];
        if let Some(c) = &cursor {
            query.push(("cursor", c));
        }
        let page = conn.request("GET", route, None, &query, None)?;
        let next = get_str(&page, "nextCursor").filter(|c| !c.is_empty()).map(str::to_string);
        let more = get(&page, "hasMore").is_some_and(truthy);
        pages.push(page);
        if !more {
            return Ok(Fetched { pages, cursor: next.or(cursor) });
        }
        match next {
            Some(n) if Some(&n) != cursor.as_ref() => cursor = Some(n),
            _ => {
                let mut error = ApiError::new("FEED_STUCK", 0);
                error.path = format!("GET {route}");
                return Err(error);
            }
        }
    }
}

/// Like `fetch_all`; an expired cursor (`CURSOR_EXPIRED`) fetches the
/// snapshot instead. The flag says that this happened.
pub fn fetch_resuming(conn: &Connection, route: &str, cursor: Option<&str>) -> Result<(Fetched, bool), ApiError> {
    match cursor.filter(|c| !c.is_empty()) {
        None => Ok((fetch_all(conn, route, None)?, false)),
        Some(c) => match fetch_all(conn, route, Some(c)) {
            Err(e) if e.code == "CURSOR_EXPIRED" => Ok((fetch_all(conn, route, None)?, true)),
            other => other.map(|f| (f, false)),
        },
    }
}

fn number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Float(x)) => *x,
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// References to Basetool ids through `POST /catalog/resolve`.
///
/// `refs`: local key and its item reference. `cache` keeps the results:
/// the id as text, or `{status, at}` for something not found, asked again
/// after `RESOLVE_AGAIN`. Returns every resolved key of the cache.
pub fn resolve_refs(
    conn: &Connection,
    kind: &str,
    refs: &[(String, Value)],
    cache: &mut Map,
    now: f64,
) -> Result<Vec<(String, String)>, ApiError> {
    let mut sorted: Vec<&(String, Value)> = refs.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted.dedup_by(|a, b| a.0 == b.0);
    let todo: Vec<&(String, Value)> = sorted
        .into_iter()
        .filter(|(key, _)| match get(cache, key) {
            Some(Value::Str(_)) => false,
            Some(Value::Object(entry)) => now - number(get(entry, "at")) >= RESOLVE_AGAIN,
            _ => true,
        })
        .collect();
    for chunk in todo.chunks(RESOLVE_MAX) {
        let body = Value::Object(vec![
            ("kind".into(), Value::str(kind)),
            ("refs".into(), Value::Array(chunk.iter().map(|(_, r)| r.clone()).collect())),
        ]);
        let answer = conn.request("POST", "/catalog/resolve", Some(&body), &[], None)?;
        for result in list(&answer, "results") {
            let Some(Value::Int(index)) = result.get("index") else { continue };
            let Some((key, _)) = index.parse::<usize>().ok().and_then(|i| chunk.get(i)) else { continue };
            let status = result.get("status").and_then(Value::as_str);
            let bt = result.get("ref").and_then(|r| r.get("bt")).and_then(Value::as_str).filter(|b| !b.is_empty());
            let entry = match (status, bt) {
                (Some("resolved"), Some(bt)) => Value::str(bt),
                _ => Value::Object(vec![
                    ("status".into(), status.map_or(Value::Null, Value::str)),
                    ("at".into(), Value::Float(now)),
                ]),
            };
            map_set(cache, key, entry);
        }
    }
    Ok(cache.iter().filter_map(|(k, v)| v.as_str().map(|bt| (k.clone(), bt.to_string()))).collect())
}

/// The storage locations of the Basetool, the only ones bookings may name:
/// lower-case name → `{name, uex}`.
#[derive(Clone, Debug, Default)]
pub struct Locations {
    pub at: f64,
    pub places: Map,
}

impl Locations {
    pub fn get(&mut self, conn: &Connection, now: f64) -> Result<&Map, ApiError> {
        if now - self.at > LOCATIONS_AGAIN || self.places.is_empty() {
            let answer = conn.request("GET", "/catalog/locations", None, &[], None)?;
            let mut places = Map::new();
            for item in list(&answer, "items") {
                let Some(name) = item.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()) else { continue };
                let kept: Map = ["name", "uex"]
                    .iter()
                    .filter_map(|k| item.get(k).filter(|v| truthy(v)).map(|v| ((*k).to_string(), v.clone())))
                    .collect();
                map_set(&mut places, &crate::text::py_strip(name).to_lowercase(), Value::Object(kept));
            }
            self.places = places;
            self.at = now;
        }
        Ok(&self.places)
    }
}

/// What became of a change set.
#[derive(Clone, Debug, PartialEq)]
pub enum Sent {
    /// Taken; the answer with `applied`, `results` …
    Answer(Map),
    /// Held until the member confirms it in the browser; nothing written.
    Held { confirm_url: Option<String>, error: ApiError },
    /// The write guard of a test (`VERSEKIT_BASETOOL_WRITE_MAX`) kept it back: nothing went out, and unlike
    /// [`Sent::Held`] nothing is remembered, so it goes as soon as the guard allows it.
    Guarded(ApiError),
}

/// One change set with a fresh `Idempotency-Key`.
pub fn send_change_set(conn: &Connection, config: &Config, route: &str, body: &Value) -> Result<Sent, ApiError> {
    match conn.request("POST", route, Some(body), &[], Some(&new_idempotency_key())) {
        Ok(answer) => Ok(Sent::Answer(answer)),
        Err(error) if error.code == MASS_CHANGE => {
            Ok(Sent::Held { confirm_url: hold::confirm_address(config, &error), error })
        }
        Err(error) if error.code == crate::testmode::WRITE_GUARD => Ok(Sent::Guarded(error)),
        Err(error) => Err(error),
    }
}
