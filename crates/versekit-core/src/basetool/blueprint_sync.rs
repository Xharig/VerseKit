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

//! The blueprints and the Basetool, the part without the net
//! (`exchange_sync.py`): the collection in the form of the contract, the plan
//! of one pass and the change sets that carry it out. Nothing here asks the
//! Basetool for anything; its answers come in as arguments, so the whole of it
//! is checked against 4.12 with made-up ones (`tools/parity_bpsync.py`).
//!
//! The rules (ADR-0218 of the Basetool and our own guard rails):
//! 1. **Nothing is deleted because it is missing.** A blueprint missing on one
//!    side is no order to remove it on the other: an incomplete state (reset
//!    collection, new installation) would clear everything. The Basetool loses
//!    only what the player ticked off **here on purpose** (`pending_removals`).
//! 2. **What was removed elsewhere does not come back silently.** It stays
//!    here (the game log proves it) but is not sent again. It lands with the
//!    player as a **conflict**, and only with consent it goes out again with
//!    `override`.
//! 3. **Start blueprints are never sent.** The Basetool gives them to every
//!    member itself and they cannot be removed there.
//! 4. **At most 500 instructions in a send.**
//!
//! A deletion mark is our own when its `removedBy.installationId` is our
//! `installation_id`; until the Basetool says so, `own_removed_keys` helps:
//! what we sent away ourselves we know.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::export::iso_time;
use crate::jsonio::{Map, Value, map_set};
use crate::paths::name_key;
use crate::text::py_strip;

pub const BATCH_MAX: usize = 500;

/// The sources that are never sent (rule 3).
pub const NEVER_SENT: [&str; 1] = ["start"];

/// Sources that come from the game logs.
pub const FROM_LOGS: [&str; 2] = ["log", "nachlese"];

/// `provenance.source` of the contract for a source of the collection.
pub fn provenance(source: &str) -> &'static str {
    match source {
        "log" | "nachlese" => "log",
        "hand" => "manual",
        "import" | "launcher" => "import",
        "start" => "default",
        _ => "other",
    }
}

/// The first `count` characters (Python slices by code point).
fn head(text: &str, count: usize) -> String {
    text.chars().take(count).collect()
}

/// A blueprint of the collection as the sync sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalBlueprint {
    pub name: String,
    /// The tag (`scRecord`), only where the name has exactly one.
    pub tag: Option<String>,
    pub source: String,
    /// The time of the collection (local time), as it stands there.
    pub time: Option<Value>,
}

/// An item reference: the tag where it is unambiguous, the name always. The
/// server takes the tag before the name and falls back to the name.
pub fn item_ref(name: &str, tag: Option<&str>) -> Value {
    let mut reference = Map::new();
    if let Some(tag) = tag.filter(|t| !t.is_empty()) {
        map_set(&mut reference, "scRecord", Value::str(head(tag, 200)));
    }
    map_set(&mut reference, "name", Value::str(head(name, 200)));
    Value::Object(reference)
}

/// The collection to `{key: blueprint}`. `tags` maps the comparison form of a
/// name to its tag (only unambiguous names).
pub fn local_blueprints(collection: &Map, tags: &HashMap<String, String>) -> BTreeMap<String, LocalBlueprint> {
    let mut out = BTreeMap::new();
    let Some(Value::Object(entries)) = collection.iter().find(|(k, _)| k == "bauplaene").map(|(_, v)| v) else {
        return out;
    };
    for (key, entry) in entries {
        let Value::Object(entry) = entry else { continue };
        let field = |name: &str| entry.iter().find(|(k, _)| k == name).map(|(_, v)| v);
        let name = match field("name") {
            Some(Value::Str(s)) => py_strip(s).to_string(),
            _ => String::new(),
        };
        if name.is_empty() {
            continue;
        }
        let source = match field("quelle") {
            Some(Value::Str(s)) if !s.is_empty() => s.clone(),
            _ => "hand".to_string(),
        };
        out.insert(
            key.clone(),
            LocalBlueprint {
                tag: tags.get(&name_key(&name)).cloned(),
                name,
                source,
                time: field("zeit").filter(|v| !matches!(v, Value::Null)).cloned(),
            },
        );
    }
    out
}

/// Only the blueprints of the own account: one that stands only in the logs of
/// another account (`foreign`, comparison form) does not go out. If it is
/// linked already it stays, else the sync would see it as removed here and
/// propose to delete it in the Basetool.
pub fn own_blueprints(
    local: &BTreeMap<String, LocalBlueprint>,
    foreign: &BTreeSet<String>,
    links: &BTreeMap<String, String>,
) -> BTreeMap<String, LocalBlueprint> {
    local
        .iter()
        .filter(|(key, entry)| {
            let from_logs = FROM_LOGS.contains(&entry.source.as_str());
            let foreign_only = foreign.contains(&name_key(&entry.name)) || foreign.contains(&name_key(key));
            !(from_logs && !links.contains_key(*key) && foreign_only)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// What follows from an answer to a send.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    pub applied: Value,
    pub conflicts: Vec<String>,
    pub unmatched: Vec<String>,
    pub ambiguous: Vec<String>,
    pub rejected: Vec<(String, String)>,
    pub cursor: Value,
}

/// The index of a result entry when it points into `targets`.
fn index_in(entry: &Value, count: usize) -> Option<usize> {
    match entry.get("index") {
        Some(Value::Int(digits)) => digits.parse::<usize>().ok().filter(|i| *i < count),
        Some(Value::Bool(b)) => Some(usize::from(*b)).filter(|i| *i < count),
        _ => None,
    }
}

/// The answer to a send (`change-result`). The server reports only what did
/// **not** work, with an index.
pub fn read_result(result: &Map, targets: &[String]) -> Read {
    let get = |key: &str| result.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let mut out = Read {
        applied: get("applied").cloned().unwrap_or_else(|| Value::int(0)),
        cursor: get("cursor").cloned().unwrap_or(Value::Null),
        conflicts: Vec::new(),
        unmatched: Vec::new(),
        ambiguous: Vec::new(),
        rejected: Vec::new(),
    };
    let Some(Value::Array(entries)) = get("results") else { return out };
    for entry in entries {
        let Some(index) = index_in(entry, targets.len()) else { continue };
        let key = targets[index].clone();
        let kind = entry.get("result").and_then(Value::as_str);
        let reason = entry.get("reason").and_then(Value::as_str);
        if reason == Some("REMOVED_ELSEWHERE") {
            out.conflicts.push(key);
        } else if kind == Some("unmatched") || reason == Some("UNMATCHED") {
            out.unmatched.push(key);
        } else if kind == Some("ambiguous") || reason == Some("AMBIGUOUS") {
            out.ambiguous.push(key);
        } else if kind == Some("rejected") {
            out.rejected.push((key, reason.unwrap_or_default().to_string()));
        }
    }
    out
}

/// Several pages of a fetch to (entries, deletion marks, next cursor). An entry
/// that stands as deleted on a later page drops out, and the other way round.
pub fn merge_pages(pages: &[Map]) -> (Vec<Value>, Vec<Value>, Option<String>) {
    let mut items: Vec<(String, Value)> = Vec::new();
    let mut stones: Vec<(String, Value)> = Vec::new();
    let mut cursor = None;
    for page in pages {
        let list = |name: &str| match page.iter().find(|(k, _)| k == name) {
            Some((_, Value::Array(items))) => items.as_slice(),
            _ => &[],
        };
        for stone in list("removed") {
            let Some(key) = stone.get("key").and_then(Value::as_str) else { continue };
            items.retain(|(k, _)| k != key);
            match stones.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = stone.clone(),
                None => stones.push((key.to_string(), stone.clone())),
            }
        }
        for item in list("items") {
            let Some(key) = item.get("key").and_then(Value::as_str) else { continue };
            match items.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = item.clone(),
                None => items.push((key.to_string(), item.clone())),
            }
            stones.retain(|(k, _)| k != key);
        }
        if let Some((_, Value::Str(next))) = page.iter().find(|(k, _)| k == "nextCursor")
            && !next.is_empty()
        {
            cursor = Some(next.clone());
        }
    }
    (items.into_iter().map(|(_, v)| v).collect(), stones.into_iter().map(|(_, v)| v).collect(), cursor)
}

/// What the last sync left behind (the blueprint part of the sync state).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlueprintState {
    /// The keys (`bt`) of the last agreed state.
    pub baseline: BTreeSet<String>,
    /// Key of the collection to `bt`.
    pub links: BTreeMap<String, String>,
    pub conflicts: BTreeMap<String, String>,
    pub pending_removals: BTreeSet<String>,
    pub overrides: BTreeSet<String>,
    pub own_removed_keys: BTreeSet<String>,
    pub installation_id: Option<String>,
}

impl BlueprintState {
    /// Reads the sync state of an installation; what is not there is empty.
    pub fn from_map(state: &Map) -> Self {
        let get = |key: &str| state.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let set_of = |key: &str| -> BTreeSet<String> {
            match get(key) {
                Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
                _ => BTreeSet::new(),
            }
        };
        let strings = |key: &str| -> BTreeMap<String, String> {
            match get(key) {
                Some(Value::Object(items)) => {
                    items.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()
                }
                _ => BTreeMap::new(),
            }
        };
        let baseline = match get("baseline") {
            Some(Value::Object(items)) => items.iter().map(|(k, _)| k.clone()).collect(),
            _ => BTreeSet::new(),
        };
        BlueprintState {
            baseline,
            links: strings("links"),
            conflicts: strings("conflicts"),
            pending_removals: set_of("pending_removals"),
            overrides: set_of("overrides"),
            own_removed_keys: set_of("own_removed_keys"),
            installation_id: match get("installation_id") {
                Some(Value::Str(id)) if !id.is_empty() => Some(id.clone()),
                _ => None,
            },
        }
    }
}

/// Did this installation remove the entry itself?
fn own_stone(stone: &Value, own_keys: &BTreeSet<String>, installation_id: Option<&str>) -> bool {
    if stone.get("key").and_then(Value::as_str).is_some_and(|k| own_keys.contains(k)) {
        return true;
    }
    let by = stone.get("removedBy").and_then(|r| r.get("installationId")).and_then(Value::as_str);
    installation_id.is_some_and(|id| by == Some(id))
}

/// One pass as a plan.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub add: Vec<String>,
    pub r#override: Vec<String>,
    /// (key of the collection, `bt`).
    pub remove: Vec<(String, String)>,
    pub conflicts: BTreeMap<String, String>,
    /// `bt` to take in here.
    pub local_add: Vec<String>,
    pub local_remove: Vec<String>,
    pub local_removed: BTreeMap<String, String>,
    pub links: BTreeMap<String, String>,
    pub unresolved: Vec<String>,
}

/// One pass after the sync guide, without the net. It works on the **`bt`
/// key of the Basetool** that `catalog/resolve` gives for every blueprint: by
/// names alone nothing could be matched safely (a wrongly matched entry would
/// show up twice).
///
/// `local`: [`local_blueprints`]. `resolved`: key to `bt`. `server`: `bt` to
/// entry, the state of the Basetool **after** fetching. `stones`: the deletion
/// marks since the last state. `resync`: after `CURSOR_EXPIRED`, then what was
/// in the last state and is missing now counts as removed elsewhere.
pub fn plan_sync(
    local: &BTreeMap<String, LocalBlueprint>,
    resolved: &BTreeMap<String, String>,
    server: &BTreeMap<String, Value>,
    stones: &[Value],
    state: &BlueprintState,
    resync: bool,
) -> Plan {
    let mut links = BTreeMap::new();
    for key in local.keys() {
        if let Some(bt) = resolved.get(key).filter(|b| !b.is_empty()).or_else(|| state.links.get(key))
            && !bt.is_empty()
        {
            links.insert(key.clone(), bt.clone());
        }
    }

    // Removed elsewhere: deletion marks of foreign channels, and after an
    // expired cursor everything that was in the last state and is missing.
    let mut elsewhere: BTreeSet<String> = stones
        .iter()
        .filter(|stone| !own_stone(stone, &state.own_removed_keys, state.installation_id.as_deref()))
        .filter_map(|stone| stone.get("key").and_then(Value::as_str).map(str::to_string))
        .collect();
    if resync {
        elsewhere.extend(
            state
                .baseline
                .iter()
                .filter(|bt| !server.contains_key(*bt) && !state.own_removed_keys.contains(*bt))
                .cloned(),
        );
    }

    let mut conflicts = BTreeMap::new();
    let mut local_remove = Vec::new();
    for (key, bt) in &links {
        if state.pending_removals.contains(key) {
            continue;
        }
        let was_conflict = state.conflicts.get(key) == Some(bt);
        if server.contains_key(bt) && !was_conflict {
            continue;
        }
        if elsewhere.contains(bt) || (was_conflict && !server.contains_key(bt)) {
            if local[key].source == "basetool" {
                // Came only from the Basetool, no proof in the game: its
                // removal counts here as well.
                local_remove.push(key.clone());
            } else {
                conflicts.insert(key.clone(), bt.clone());
            }
        }
    }

    // Removed here, linked at the last sync, still there: the player decides.
    // Never delete silently, never bring back silently.
    let mut local_removed = BTreeMap::new();
    for (key, bt) in &state.links {
        if !local.contains_key(key) && server.contains_key(bt) && !state.pending_removals.contains(key) {
            local_removed.insert(key.clone(), bt.clone());
        }
    }

    let (mut add, mut r#override, mut unresolved) = (Vec::new(), Vec::new(), Vec::new());
    for (key, entry) in local {
        if NEVER_SENT.contains(&entry.source.as_str()) || entry.source == "basetool" {
            continue;
        }
        if state.pending_removals.contains(key) {
            continue;
        }
        let Some(bt) = links.get(key) else {
            unresolved.push(key.clone());
            continue;
        };
        if server.contains_key(bt) {
            continue;
        }
        if conflicts.contains_key(key) {
            if state.overrides.contains(key) {
                r#override.push(key.clone());
            }
            continue;
        }
        add.push(key.clone());
    }

    let mut remove = Vec::new();
    for key in &state.pending_removals {
        let bt = state.links.get(key).or_else(|| links.get(key));
        let Some(item) = bt.and_then(|bt| server.get(bt)) else { continue };
        let is_default = matches!(item.get("isDefault"), Some(v) if crate::basetool::error::truthy(v));
        if !is_default && let Some(bt) = bt {
            remove.push((key.clone(), bt.clone()));
        }
    }

    let taken: BTreeSet<&String> = links.values().chain(local_removed.values()).collect();
    let local_add: Vec<String> = server.keys().filter(|bt| !taken.contains(bt)).cloned().collect();

    Plan { add, r#override, remove, conflicts, local_add, local_remove, local_removed, links, unresolved }
}

/// The add operation of one blueprint, `bt` and name in the reference.
fn add_op(entry: &LocalBlueprint, bt: &str, op_id: &str, override_it: bool) -> Value {
    let mut op = Map::new();
    map_set(&mut op, "opId", Value::str(op_id));
    map_set(&mut op, "op", Value::str("add"));
    map_set(&mut op, "ref", item_ref(&entry.name, entry.tag.as_deref()));
    let acquired = iso_time(entry.time.as_ref());
    if let Some(acquired) = &acquired {
        map_set(&mut op, "acquiredAt", Value::str(acquired));
    }
    let mut provenance_map = Map::new();
    map_set(&mut provenance_map, "source", Value::str(provenance(&entry.source)));
    if let Some(acquired) = &acquired {
        map_set(&mut provenance_map, "observedAt", Value::str(acquired));
    }
    map_set(&mut op, "provenance", Value::Object(provenance_map));
    if override_it {
        map_set(&mut op, "override", Value::Bool(true));
    }
    let mut reference = Map::new();
    map_set(&mut reference, "bt", Value::str(bt));
    map_set(&mut reference, "name", Value::str(head(&entry.name, 200)));
    map_set(&mut op, "ref", Value::Object(reference));
    Value::Object(op)
}

/// The plan as sends: `(change set, targets)`, at most 500 each. Adding
/// carries `bt` AND the name; the server takes `bt`.
pub fn sync_change_sets(plan: &Plan, local: &BTreeMap<String, LocalBlueprint>) -> Vec<(Value, Vec<String>)> {
    let mut ops: Vec<Value> = Vec::new();
    let mut targets: Vec<String> = Vec::new();
    for (n, key) in plan.add.iter().enumerate() {
        ops.push(add_op(&local[key], &plan.links[key], &format!("a{}", n + 1), false));
        targets.push(key.clone());
    }
    for (n, key) in plan.r#override.iter().enumerate() {
        ops.push(add_op(&local[key], &plan.links[key], &format!("o{}", n + 1), true));
        targets.push(key.clone());
    }
    for (n, (key, bt)) in plan.remove.iter().enumerate() {
        let mut op = Map::new();
        map_set(&mut op, "opId", Value::str(format!("r{}", n + 1)));
        map_set(&mut op, "op", Value::str("remove"));
        map_set(&mut op, "key", Value::str(bt));
        ops.push(Value::Object(op));
        targets.push(key.clone());
    }
    ops.chunks(BATCH_MAX)
        .zip(targets.chunks(BATCH_MAX))
        .map(|(ops, targets)| {
            let mut set = Map::new();
            map_set(&mut set, "ops", Value::Array(ops.to_vec()));
            (Value::Object(set), targets.to_vec())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bp(source: &str) -> LocalBlueprint {
        LocalBlueprint { name: "Name".into(), tag: None, source: source.into(), time: None }
    }

    fn item(bt: &str) -> (String, Value) {
        (bt.to_string(), Value::Object(vec![("key".into(), Value::str(bt))]))
    }

    #[test]
    fn the_sources_map_to_the_contract_and_start_is_never_sent() {
        assert_eq!(provenance("nachlese"), "log");
        assert_eq!(provenance("launcher"), "import");
        assert_eq!(provenance("etwas"), "other");
        let local: BTreeMap<_, _> =
            [("a".to_string(), bp("start")), ("b".to_string(), bp("log"))].into_iter().collect();
        let resolved: BTreeMap<_, _> =
            [("a".to_string(), "bt-a".to_string()), ("b".to_string(), "bt-b".to_string())].into_iter().collect();
        let plan = plan_sync(&local, &resolved, &BTreeMap::new(), &[], &BlueprintState::default(), false);
        assert_eq!(plan.add, ["b"], "start blueprints are not sent");
    }

    #[test]
    fn nothing_is_deleted_because_it_is_missing_and_only_ticked_ones_are_removed() {
        let local: BTreeMap<_, _> = [("a".to_string(), bp("log"))].into_iter().collect();
        let server: BTreeMap<_, _> = [item("bt-a"), item("bt-b")].into_iter().collect();
        let resolved: BTreeMap<_, _> = [("a".to_string(), "bt-a".to_string())].into_iter().collect();
        let mut state = BlueprintState::default();
        let plan = plan_sync(&local, &resolved, &server, &[], &state, false);
        assert!(plan.remove.is_empty(), "bt-b is missing here: that is no order to delete it");
        assert_eq!(plan.local_add, ["bt-b"]);
        state.pending_removals.insert("a".into());
        state.links.insert("a".into(), "bt-a".into());
        let plan = plan_sync(&local, &resolved, &server, &[], &state, false);
        assert_eq!(plan.remove, [("a".to_string(), "bt-a".to_string())]);
    }

    #[test]
    fn a_blueprint_removed_elsewhere_becomes_a_conflict_and_goes_out_again_only_with_consent() {
        let local: BTreeMap<_, _> = [("a".to_string(), bp("log"))].into_iter().collect();
        let resolved: BTreeMap<_, _> = [("a".to_string(), "bt-a".to_string())].into_iter().collect();
        let stone = Value::Object(vec![("key".into(), Value::str("bt-a"))]);
        let mut state = BlueprintState::default();
        let plan = plan_sync(&local, &resolved, &BTreeMap::new(), std::slice::from_ref(&stone), &state, false);
        assert_eq!(plan.conflicts.get("a").map(String::as_str), Some("bt-a"));
        assert!(plan.add.is_empty() && plan.r#override.is_empty());
        state.overrides.insert("a".into());
        let plan = plan_sync(&local, &resolved, &BTreeMap::new(), std::slice::from_ref(&stone), &state, false);
        assert_eq!(plan.r#override, ["a"]);
        // Our own deletion mark is no conflict.
        let mine = Value::Object(vec![
            ("key".into(), Value::str("bt-a")),
            ("removedBy".into(), Value::Object(vec![("installationId".into(), Value::str("inst"))])),
        ]);
        let state = BlueprintState { installation_id: Some("inst".into()), ..BlueprintState::default() };
        let plan = plan_sync(&local, &resolved, &BTreeMap::new(), std::slice::from_ref(&mine), &state, false);
        assert!(plan.conflicts.is_empty());
        assert_eq!(plan.add, ["a"]);
    }

    #[test]
    fn the_change_sets_carry_bt_and_name_and_hold_at_most_five_hundred() {
        let local: BTreeMap<String, LocalBlueprint> = (0..501).map(|n| (format!("k{n:04}"), bp("hand"))).collect();
        let links: BTreeMap<String, String> = local.keys().map(|k| (k.clone(), format!("bt-{k}"))).collect();
        let plan = Plan { add: local.keys().cloned().collect(), links, ..Plan::default() };
        let sets = sync_change_sets(&plan, &local);
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].1.len(), 500);
        let first = &sets[0]
            .0
            .get("ops")
            .and_then(|o| match o {
                Value::Array(a) => a.first(),
                _ => None,
            })
            .cloned();
        let first = first.as_ref().expect("an op");
        assert_eq!(first.get("opId").and_then(Value::as_str), Some("a1"));
        assert_eq!(first.get("ref").and_then(|r| r.get("bt")).and_then(Value::as_str), Some("bt-k0000"));
        assert_eq!(first.get("provenance").and_then(|p| p.get("source")).and_then(Value::as_str), Some("manual"));
    }

    #[test]
    fn what_did_not_work_is_read_from_the_answer() {
        let answer = crate::basetool::connection::parse_answer(
            br#"{"applied": 1, "results": [
                {"index": 0, "result": "rejected", "reason": "REMOVED_ELSEWHERE"},
                {"index": 1, "result": "unmatched"},
                {"index": 2, "reason": "AMBIGUOUS"},
                {"index": 3, "result": "rejected", "reason": "X"},
                {"index": 9, "result": "rejected"}]}"#,
        );
        let targets: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let read = read_result(&answer, &targets);
        assert_eq!(read.conflicts, ["a"]);
        assert_eq!(read.unmatched, ["b"]);
        assert_eq!(read.ambiguous, ["c"]);
        assert_eq!(read.rejected, [("d".to_string(), "X".to_string())]);
    }

    #[test]
    fn pages_are_merged_with_the_later_one_winning() {
        let page = |text: &str| match crate::jsonio::loads(text) {
            Ok(Value::Object(map)) => map,
            other => panic!("{other:?}"),
        };
        let pages = [
            page(r#"{"items": [{"key": "a"}, {"key": "b"}], "nextCursor": "c1"}"#),
            page(r#"{"removed": [{"key": "a"}], "items": [{"key": "c"}], "nextCursor": "c2"}"#),
            page(r#"{"items": [{"key": "a"}]}"#),
        ];
        let (items, stones, cursor) = merge_pages(&pages);
        let keys: Vec<&str> = items.iter().filter_map(|i| i.get("key").and_then(Value::as_str)).collect();
        assert_eq!(keys, ["b", "c", "a"], "a came back after its mark");
        assert!(stones.is_empty());
        assert_eq!(cursor.as_deref(), Some("c2"));
    }
}
