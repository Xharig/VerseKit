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

//! The area blueprints of the sync (`basetool_sync._sync_blueprints`,
//! `_pull`, `_resolve`, `_foreign_only`): fetch, compare, send, fetch again,
//! take over what came from there. The plan itself is in
//! [`blueprint_sync`](crate::basetool::blueprint_sync).
//!
//! **Who writes the collection.** In 4.12 only the watcher writes it and the
//! sync hands changes over (`watcher.basetool_apply`). Here the host does it
//! ([`BlueprintHost::apply`]), as the window does for a tick in the list or the
//! removal of foreign blueprints; the watcher in the overlay takes the file over
//! before it writes its next find (`Run::refresh`), so neither takes back what
//! the other did. A change made for the sync is no own change that would ask for
//! another sync ([`Pass::apply_local`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use crate::basetool::area_support::{put_object, store_busy, take_object, text_array, text_map, texts};
use crate::basetool::blueprint_sync::{self as plan_mod, BlueprintState, LocalBlueprint};
use crate::basetool::connection::{get, get_str};
use crate::basetool::error::ApiError;
use crate::basetool::feed::{self, Sent, fetch_all, list};
use crate::basetool::hold;
use crate::basetool::scopes::{ROUTE_BLUEPRINTS, SETTING_BLUEPRINTS};
use crate::basetool::sync::{Area, Pass};
use crate::collection::{self, Catalog, Collection};
use crate::export::{self, FileExports};
use crate::jsonio::{Map, Value, map_set};
use crate::logsource::{ALL_ACCOUNTS, LogSource};
use crate::paths::{Paths, name_key};
use crate::phrases::{self, BackRoute, NoArchive};

/// How long the set of blueprints that only other accounts found is kept
/// (reading all backups of the logs is expensive).
pub const FOREIGN_AGAIN: f64 = 3600.0;

/// What fetching gives: the stand over there, the deletion marks since the last time, the cursor and whether the
/// cursor had expired (then it is a new snapshot compared with the last stand).
type Pulled = (BTreeMap<String, Value>, Vec<Value>, Option<String>, bool);

/// What the sync does to the collection.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// A blueprint from the Basetool, with the time it was acquired there
    /// (local time of the collection).
    Add { name: String, time: Option<String> },
    /// A blueprint that came only from the Basetool and is gone there.
    Remove { key: String },
}

/// The collection and what the sync needs around it.
pub trait BlueprintHost: Send + Sync {
    /// The collection (`bestand.json`).
    fn load(&self) -> Map;
    /// The key each name gets in the collection (the comparison form of its
    /// catalog name), in the order of `names`. The catalog is read once for all.
    fn keys_of(&self, names: &[String]) -> Vec<String>;
    /// The tag of every name that has exactly one.
    fn tags(&self) -> std::collections::HashMap<String, String>;
    /// The comparison forms of the blueprints that stand only in the logs of
    /// accounts other than `own`. Reads all backups: slow.
    fn foreign_only(&self, own: &str) -> BTreeSet<String>;
    /// Makes the changes and saves the collection when something changed.
    /// Adding keeps the best source it has; removing takes out only an entry
    /// whose source is `basetool` (what a log proves stays). Gives whether the
    /// collection changed. Gives `false` only when the collection could not be
    /// written; nothing to change is `true`.
    fn apply(&self, changes: &[Change]) -> bool;
}

/// How long the names of the catalog are kept (reading them is expensive).
const CATALOG_KEPT: f64 = 300.0;

/// The catalog read at most every [`CATALOG_KEPT`] seconds, for the many names of one pass.
struct KeptCatalog {
    inner: Arc<dyn Catalog + Send + Sync>,
    kept: Mutex<Option<(std::time::Instant, collection::Known)>>,
}

impl Catalog for KeptCatalog {
    fn known(&self) -> Result<collection::Known, String> {
        let mut kept = self.kept.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((at, known)) = kept.as_ref()
            && at.elapsed().as_secs_f64() < CATALOG_KEPT
            && !known.is_empty()
        {
            return Ok(known.clone());
        }
        let known = self.inner.known()?;
        *kept = Some((std::time::Instant::now(), known.clone()));
        Ok(known)
    }
}

/// The collection in the data folder.
pub struct FileBlueprintHost {
    paths: Arc<Paths>,
    catalog: KeptCatalog,
    version: &'static str,
}

impl FileBlueprintHost {
    /// `catalog` knows the names of the blueprints (to name a blueprint from the
    /// Basetool the way the collection does).
    pub fn new(paths: Arc<Paths>, catalog: Arc<dyn Catalog + Send + Sync>) -> FileBlueprintHost {
        let catalog = KeptCatalog { inner: catalog, kept: Mutex::new(None) };
        FileBlueprintHost { paths, catalog, version: env!("CARGO_PKG_VERSION") }
    }

    fn with_collection<R>(&self, work: impl FnOnce(&Collection) -> R) -> R {
        let names = BackRoute { paths: &self.paths, archive: &NoArchive };
        let exports = FileExports { paths: &self.paths, version: self.version, clock: export::now_unix };
        let collection = Collection {
            paths: &self.paths,
            catalog: &self.catalog,
            names: &names,
            exports: &exports,
            clock: collection::now_text,
        };
        work(&collection)
    }
}

impl BlueprintHost for FileBlueprintHost {
    fn load(&self) -> Map {
        self.with_collection(|c| c.load())
    }

    fn keys_of(&self, names: &[String]) -> Vec<String> {
        self.with_collection(|c| {
            let known = c.catalog.known().ok();
            names.iter().map(|name| name_key(&c.catalog_name(name, known.as_ref()))).collect()
        })
    }

    fn tags(&self) -> std::collections::HashMap<String, String> {
        export::unique_tags(&self.paths)
    }

    fn foreign_only(&self, own: &str) -> BTreeSet<String> {
        let source = LogSource::new(&self.paths, LogSource::sources_of(&self.paths));
        let pattern = phrases::pattern(&source.sources.collect().0);
        source.foreign_only_blueprints(&pattern, Some(own)).iter().map(|name| name_key(name)).collect()
    }

    fn apply(&self, changes: &[Change]) -> bool {
        self.with_collection(|c| {
            // Read, change and write as one step: the window and the watcher write the same file.
            let _lock = crate::jsonio::lock_saves();
            let mut data = c.load();
            let mut changed = false;
            for change in changes {
                match change {
                    Change::Add { name, time } => {
                        changed |= c.add(&mut data, name, "basetool", time.as_deref());
                    }
                    Change::Remove { key } => {
                        let only_basetool = collection::blueprints(&data)
                            .iter()
                            .find(|(k, _)| k == key)
                            .and_then(|(_, entry)| entry.get("quelle"))
                            .and_then(Value::as_str)
                            == Some("basetool");
                        if only_basetool {
                            changed |= collection::remove(&mut data, key);
                        }
                    }
                }
            }
            if changed {
                return c.save(&mut data);
            }
            true
        })
    }
}

/// `YYYY-MM-DDTHH:MM:SS…` in UTC to the local time of the collection
/// (`YYYY-MM-DD HH:MM:SS`); `None` where it cannot be read.
pub fn local_time_of(iso: &str) -> Option<String> {
    let head = iso.get(..19)?;
    let number = |range: std::ops::Range<usize>| head.get(range)?.parse::<i64>().ok();
    if head.as_bytes().get(4) != Some(&b'-')
        || head.as_bytes().get(7) != Some(&b'-')
        || head.as_bytes().get(10) != Some(&b'T')
        || head.as_bytes().get(13) != Some(&b':')
        || head.as_bytes().get(16) != Some(&b':')
    {
        return None;
    }
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    // `time.strptime` knows the length of the month and the seconds up to 61.
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let month_days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if !(1..=12).contains(&month) || !(1..=month_days).contains(&day) || hour > 23 || minute > 59 || second > 61 {
        return None;
    }
    // Days from the civil date by the usual era calculation.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let stamp = days * 86_400 + hour * 3600 + minute * 60 + second;
    let local = crate::formats::local_time(stamp)?;
    Some(crate::formats::strftime("%Y-%m-%d %H:%M:%S", &local))
}

/// The blueprints area.
pub struct BlueprintArea {
    host: Arc<dyn BlueprintHost>,
    /// The set of foreign-only names with the time and the account it was made for.
    foreign: Mutex<(f64, Option<String>, BTreeSet<String>)>,
}

impl BlueprintArea {
    pub fn new(host: Arc<dyn BlueprintHost>) -> BlueprintArea {
        BlueprintArea { host, foreign: Mutex::new((0.0, None, BTreeSet::new())) }
    }

    /// Blueprints that stand only in logs of other accounts (comparison form).
    /// Reading all backups is expensive: the result holds for an hour and is
    /// made anew when the account changes.
    fn foreign_only(&self, own: Option<&str>, now: f64) -> BTreeSet<String> {
        let mut cache = self.foreign.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.1.as_deref() == own && now - cache.0 < FOREIGN_AGAIN && cache.0 > 0.0 {
            return cache.2.clone();
        }
        let names = match own {
            Some(own) if !own.is_empty() && own != ALL_ACCOUNTS => self.host.foreign_only(own),
            _ => BTreeSet::new(),
        };
        *cache = (now, own.map(str::to_string), names.clone());
        names
    }

    /// Fetching: the stand of the Basetool, the deletion marks since the last
    /// time, the cursor, and whether this was a new snapshot after an expired
    /// cursor (`_pull`).
    fn pull(conn: &crate::basetool::connection::Connection, state: &Map) -> Result<Pulled, ApiError> {
        let baseline: Map = match get(state, "baseline") {
            Some(Value::Object(inner)) => inner.clone(),
            _ => Map::new(),
        };
        let cursor = get_str(state, "cursor").filter(|c| !c.is_empty()).map(str::to_string);
        let keyed = |items: Vec<Value>| -> BTreeMap<String, Value> {
            items
                .into_iter()
                .filter_map(|i| i.get("key").and_then(Value::as_str).map(|k| (k.to_string(), i.clone())))
                .collect()
        };
        let Some(cursor) = cursor else {
            let fetched = fetch_all(conn, "/me/blueprints", None)?;
            let (items, _stones, cursor) = plan_mod::merge_pages(&fetched.pages);
            return Ok((keyed(items), Vec::new(), cursor, false));
        };
        match fetch_all(conn, "/me/blueprints", Some(&cursor)) {
            Err(error) if error.code == "CURSOR_EXPIRED" => {
                let fetched = fetch_all(conn, "/me/blueprints", None)?;
                let (items, _stones, cursor) = plan_mod::merge_pages(&fetched.pages);
                // After an expired cursor the stand is compared with the LAST one: that is expressly no first sync.
                Ok((keyed(items), Vec::new(), cursor, !baseline.is_empty()))
            }
            Err(error) => Err(error),
            Ok(fetched) => {
                let (items, stones, cursor) = plan_mod::merge_pages(&fetched.pages);
                let mut server: BTreeMap<String, Value> = baseline.into_iter().collect();
                for stone in &stones {
                    if let Some(key) = stone.get("key").and_then(Value::as_str) {
                        server.remove(key);
                    }
                }
                for item in items {
                    if let Some(key) = item.get("key").and_then(Value::as_str) {
                        server.insert(key.to_string(), item.clone());
                    }
                }
                Ok((server, stones, cursor, false))
            }
        }
    }

    /// `bt` for every blueprint that has none yet, through `catalog/resolve`
    /// (`_resolve`). What the Basetool does not know is asked again after a day.
    fn resolve(
        conn: &crate::basetool::connection::Connection,
        local: &BTreeMap<String, LocalBlueprint>,
        state: &mut Map,
        now: f64,
    ) -> Result<BTreeMap<String, String>, ApiError> {
        let mut cache = take_object(state, "resolved");
        let refs: Vec<(String, Value)> = local
            .iter()
            .filter(|(_, entry)| !plan_mod::NEVER_SENT.contains(&entry.source.as_str()))
            .map(|(key, entry)| (key.clone(), plan_mod::item_ref(&entry.name, entry.tag.as_deref())))
            .collect();
        let found = feed::resolve_refs(conn, "BLUEPRINT", &refs, &mut cache, now);
        put_object(state, "resolved", cache);
        Ok(found?.into_iter().collect())
    }
}

impl Area for BlueprintArea {
    fn name(&self) -> &'static str {
        "blueprints"
    }

    fn runs(&self, ready: &[&'static str], _capabilities: &[String]) -> bool {
        ready.contains(&SETTING_BLUEPRINTS)
    }

    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError> {
        let conn = pass.conn;
        // 1) Fetch.
        let (mut server, stones, mut cursor, resync) = Self::pull(conn, pass.state)?;

        // 2) Compare.
        let state_view = BlueprintState::from_map(pass.state);
        let collection = self.host.load();
        let own = (pass.own_account)();
        let foreign = self.foreign_only(own.as_deref(), pass.now);
        let local_all = plan_mod::local_blueprints(&collection, &self.host.tags());
        let local = plan_mod::own_blueprints(&local_all, &foreign, &state_view.links);
        let resolved = Self::resolve(conn, &local, pass.state, pass.now)?;
        let plan = plan_mod::plan_sync(&local, &resolved, &server, &stones, &state_view, resync);

        // 3) Send.
        let mut counts = Map::new();
        let (mut added, mut removed, mut unchanged, mut unmatched, mut ambiguous) = (0_i64, 0_i64, 0_i64, 0_i64, 0_i64);
        let mut paused = false;
        let mut sent_removals: Vec<String> = Vec::new();
        let mut new_conflicts: BTreeMap<String, String> = plan.conflicts.clone();
        // The keys of the sends the Basetool took: only their removals and overrides are done.
        let mut posted: BTreeSet<String> = BTreeSet::new();
        let mut refused: Vec<Value> = Vec::new();
        let mut change_sets = plan_mod::sync_change_sets(&plan, &local);
        crate::basetool::pacing::clear_expired_wait(pass.state, pass.now);
        if crate::basetool::pacing::wait_until(pass.state, pass.now).is_some()
            || !hold::may_send(pass.state, pass.confirmations, ROUTE_BLUEPRINTS, &change_sets, pass.now, pass.release_now)
        {
            paused = true;
            change_sets.clear();
        }
        let all_sets = change_sets.clone();
        for (number, (change_set, targets)) in all_sets.iter().enumerate() {
            let sent = match feed::send_change_set(conn, conn.config(), ROUTE_BLUEPRINTS, change_set) {
                Ok(sent) => sent,
                Err(error) if crate::basetool::pacing::wait_after(pass.state, pass.now, &error).is_some() => {
                    error.record("basetool_sync.send");
                    paused = true;
                    break;
                }
                Err(error) => return Err(error),
            };
            let result = match sent {
                Sent::Held { confirm_url, error: _ } => {
                    hold::hold(
                        pass.state,
                        pass.confirmations,
                        ROUTE_BLUEPRINTS,
                        &all_sets[number..],
                        confirm_url,
                        &refused,
                        pass.now,
                    );
                    paused = true;
                    break;
                }
                Sent::Guarded(_) => {
                    // The write guard of a test: kept back like a held send, but not remembered as one.
                    paused = true;
                    break;
                }
                Sent::Answer(result) => result,
            };
            posted.extend(targets.iter().cloned());
            let read = plan_mod::read_result(&result, targets);
            unchanged += crate::basetool::area_support::count_of(&result, "unchanged");
            unmatched += read.unmatched.len() as i64;
            ambiguous += read.ambiguous.len() as i64;
            for key in &read.conflicts {
                if let Some(bt) = plan.links.get(key) {
                    new_conflicts.insert(key.clone(), bt.clone());
                }
            }
            let same: BTreeSet<String> = list(&result, "results")
                .iter()
                .filter(|e| e.get("result").and_then(Value::as_str) == Some("unchanged"))
                .filter_map(|e| match e.get("index") {
                    Some(Value::Int(n)) => n.parse::<usize>().ok().and_then(|i| targets.get(i)).cloned(),
                    _ => None,
                })
                .collect();
            let mut failed: BTreeSet<String> = BTreeSet::new();
            failed.extend(read.conflicts.iter().cloned());
            failed.extend(read.unmatched.iter().cloned());
            failed.extend(read.ambiguous.iter().cloned());
            failed.extend(read.rejected.iter().map(|(k, _)| k.clone()));
            failed.extend(same);
            refused.extend(crate::basetool::area_support::refused_ops(change_set, targets, &failed));
            let ops: &[Value] = match change_set.get("ops") {
                Some(Value::Array(ops)) => ops,
                _ => &[],
            };
            for (op, key) in ops.iter().zip(targets) {
                if failed.contains(key) {
                    continue;
                }
                if op.get("op").and_then(Value::as_str) == Some("remove") {
                    if let Some(bt) = op.get("key").and_then(Value::as_str) {
                        sent_removals.push(bt.to_string());
                    }
                    removed += 1;
                } else {
                    added += 1;
                }
            }
        }

        // 4) Fetch once more: the new agreed stand. ⚠ The feed from the cursor just
        // fetched builds on the stand just fetched, not on the old one, or what step
        // 1 brought would be missing.
        if added > 0 || removed > 0 || !plan.r#override.is_empty() {
            map_set(
                pass.state,
                "baseline",
                Value::Object(server.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
            );
            map_set(pass.state, "cursor", cursor.as_ref().map_or(Value::Null, Value::str));
            let (again, _stones, next, _resync) = Self::pull(conn, pass.state)?;
            server = again;
            cursor = next;
        }

        // 5) Take over here what came from there or was removed elsewhere.
        let mut changes: Vec<Change> = Vec::new();
        let mut new_links: BTreeMap<String, String> = plan.links.clone();
        let mut resolved_state = take_object(pass.state, "resolved");
        let mut taken: Vec<(String, String, Option<String>)> = Vec::new();
        for bt in &plan.local_add {
            let item = server.get(bt);
            let name = item
                .and_then(|i| i.get("ref"))
                .and_then(|r| r.get("name"))
                .and_then(Value::as_str)
                .map(|n| crate::text::py_strip(n).to_string())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let time = item.and_then(|i| i.get("acquiredAt")).and_then(Value::as_str).and_then(local_time_of);
            taken.push((bt.clone(), name, time));
        }
        let names: Vec<String> = taken.iter().map(|(_, name, _)| name.clone()).collect();
        for ((bt, name, time), key) in taken.into_iter().zip(self.host.keys_of(&names)) {
            changes.push(Change::Add { name, time });
            new_links.insert(key.clone(), bt.clone());
            map_set(&mut resolved_state, &key, Value::str(&bt));
        }
        put_object(pass.state, "resolved", resolved_state);
        for key in &plan.local_remove {
            changes.push(Change::Remove { key: key.clone() });
            new_links.remove(key);
        }
        if !changes.is_empty() {
            let host = self.host.clone();
            // A collection that could not be written ends the pass without its state: the links would
            // otherwise claim blueprints that never stood here, and the next pass would ask about them.
            if !pass.apply_local(|| host.apply(&changes)) {
                return Err(store_busy("collection not written"));
            }
        }

        let done: BTreeSet<String> =
            plan.remove.iter().filter(|(k, _)| posted.contains(k)).map(|(k, _)| k.clone()).collect();
        // Releases that are done over there already (a send confirmed in the browser): removed is what is no
        // longer in the stand; taken back is what is in it again.
        let mut linked: BTreeMap<String, String> = text_map(pass.state, "links");
        linked.extend(new_links.iter().map(|(k, v)| (k.clone(), v.clone())));
        let old_pending: BTreeSet<String> = texts(pass.state, "pending_removals").into_iter().collect();
        let pending: BTreeSet<String> = old_pending
            .difference(&done)
            .filter(|k| linked.get(*k).is_some_and(|bt| server.contains_key(bt)))
            .cloned()
            .collect();
        let old_overrides: BTreeSet<String> = texts(pass.state, "overrides").into_iter().collect();
        let sent_override: BTreeSet<&String> = plan.r#override.iter().filter(|k| posted.contains(*k)).collect();
        let overrides: BTreeSet<String> = old_overrides
            .iter()
            .filter(|k| !sent_override.contains(*k))
            .filter(|k| !linked.get(*k).is_some_and(|bt| server.contains_key(bt)))
            .cloned()
            .collect();
        let mut own_removed: BTreeSet<String> = texts(pass.state, "own_removed_keys").into_iter().collect();
        own_removed.extend(sent_removals.iter().cloned());

        let baseline: Map = server
            .iter()
            .map(|(bt, item)| {
                let reference = match item.get("ref") {
                    Some(v @ Value::Object(_)) => v.clone(),
                    _ => Value::Object(Map::new()),
                };
                let is_default = item.get("isDefault").is_some_and(crate::basetool::error::truthy);
                (
                    bt.clone(),
                    Value::Object(vec![
                        ("key".into(), Value::str(bt)),
                        ("ref".into(), reference),
                        ("isDefault".into(), Value::Bool(is_default)),
                    ]),
                )
            })
            .collect();
        let kept_links: Map = new_links
            .iter()
            .filter(|(k, bt)| server.contains_key(*bt) || new_conflicts.contains_key(*k))
            .map(|(k, bt)| (k.clone(), Value::str(bt)))
            .collect();
        let kept_conflicts: Map = new_conflicts
            .iter()
            .filter(|(_, bt)| !server.contains_key(*bt))
            .map(|(k, bt)| (k.clone(), Value::str(bt)))
            .collect();
        let local_removed: BTreeMap<String, String> = plan
            .local_removed
            .iter()
            .filter(|(_, bt)| server.contains_key(*bt))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        map_set(pass.state, "cursor", cursor.as_ref().map_or(Value::Null, Value::str));
        map_set(pass.state, "baseline", Value::Object(baseline));
        map_set(pass.state, "links", Value::Object(kept_links));
        map_set(pass.state, "conflicts", Value::Object(kept_conflicts));
        map_set(
            pass.state,
            "local_removed",
            Value::Object(local_removed.iter().map(|(k, v)| (k.clone(), Value::str(v))).collect()),
        );
        map_set(pass.state, "pending_removals", text_array(pending.iter()));
        map_set(pass.state, "overrides", text_array(overrides.iter()));
        map_set(pass.state, "own_removed_keys", text_array(own_removed.iter()));
        // Removed here and released over there stays linked until it is really gone over there, or the next
        // pass would take it for new.
        let mut links_now = take_object(pass.state, "links");
        for (key, bt) in &local_removed {
            map_set(&mut links_now, key, Value::str(bt));
        }
        put_object(pass.state, "links", links_now);

        for (name, value) in [
            ("added", added),
            ("removed", removed),
            ("unchanged", unchanged),
            ("unmatched", unmatched),
            ("ambiguous", ambiguous),
        ] {
            map_set(&mut counts, name, Value::int(value));
        }
        if paused {
            map_set(&mut counts, "paused", Value::int(1));
        }
        map_set(&mut counts, "local_added", Value::int(plan.local_add.len() as i64));
        map_set(&mut counts, "local_removed", Value::int(plan.local_remove.len() as i64));
        map_set(&mut counts, "unresolved", Value::int(plan.unresolved.len() as i64));
        Ok(counts)
    }

    fn forget(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::local_time_of;

    #[test]
    fn a_time_of_the_basetool_becomes_a_local_time_of_the_collection() {
        let text = local_time_of("2026-03-14T09:26:53.589Z").expect("a time");
        // The zone of the machine moves the clock, never the shape.
        assert_eq!(text.len(), 19, "{text}");
        assert_eq!(text.as_bytes()[10], b' ');
        assert!(text.starts_with("2026-03-1"), "{text}");
        // A leap second as `time.strptime` takes it, and a leap day only in a leap year.
        assert!(local_time_of("2026-06-30T23:59:61Z").is_some());
        assert!(local_time_of("2028-02-29T12:00:00Z").is_some());
    }

    #[test]
    fn a_time_that_does_not_exist_gives_nothing() {
        for bad in [
            "2026-02-30T12:00:00Z",
            "2027-02-29T12:00:00Z",
            "2100-02-29T12:00:00Z",
            "2026-04-31T12:00:00Z",
            "2026-13-01T12:00:00Z",
            "2026-00-10T12:00:00Z",
            "2026-01-10T24:00:00Z",
            "2026-01-10T12:60:00Z",
            "2026-01-10T12:00:62Z",
            "2026-01-10 12:00:00",
            "gestern",
            "",
        ] {
            assert_eq!(local_time_of(bad), None, "{bad}");
        }
    }
}
