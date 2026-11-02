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

//! What only the player may decide when the two sides differ
//! (`basetool_sync.release_removals` … `ships_override`): things removed on
//! one side only, and amounts that differ.
//!
//! Each decision is a change of the sync state that can be applied again and
//! again: a pass that already loaded the state applies it once more before it
//! writes (`SyncHandle::decide`). Nothing is removed on the other side without
//! the player's word.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::basetool::sync::Change;
use crate::jsonio::{Map, Value, map_set};

/// The object under `key`, made if it is missing or something else.
fn object_mut<'a>(map: &'a mut Map, key: &str) -> &'a mut Map {
    if !matches!(map.iter().find(|(k, _)| k == key), Some((_, Value::Object(_)))) {
        map_set(map, key, Value::Object(Map::new()));
    }
    match map.iter_mut().find(|(k, _)| k == key) {
        Some((_, Value::Object(inner))) => inner,
        _ => unreachable!("the object was just made"),
    }
}

/// `container[key] = sorted(set(container.get(key) or ()) | set(values))`.
fn add_to(container: &mut Map, key: &str, values: &[String]) {
    let mut all: BTreeSet<String> = match container.iter().find(|(k, _)| k == key) {
        Some((_, Value::Array(items))) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => BTreeSet::new(),
    };
    all.extend(values.iter().cloned());
    map_set(container, key, Value::Array(all.into_iter().map(Value::Str).collect()));
}

/// Takes `keys` out of the object under `key`, if there is one.
fn remove_from(container: &mut Map, key: &str, keys: &[String]) {
    if let Some((_, Value::Object(inner))) = container.iter_mut().find(|(k, _)| k == key) {
        inner.retain(|(k, _)| !keys.contains(k));
    }
}

/// Blueprints removed here: also remove them in the Basetool. Only now they
/// are sent.
pub fn release_removals(keys: Vec<String>) -> Change {
    Arc::new(move |state: &mut Map| add_to(state, "pending_removals", &keys))
}

/// Blueprints removed in the Basetool: put them back anyway.
pub fn override_conflicts(keys: Vec<String>) -> Change {
    Arc::new(move |state: &mut Map| add_to(state, "overrides", &keys))
}

/// Take blueprints removed here back. Returns the change and the names the
/// caller adds to the collection again.
pub fn keep_removed(state: &Map, keys: &[String]) -> (Change, Vec<String>) {
    let removed = state.iter().find(|(k, _)| k == "local_removed").and_then(|(_, v)| v.as_object());
    let baseline = state.iter().find(|(k, _)| k == "baseline").and_then(|(_, v)| v.as_object());
    let mut names = Vec::new();
    for key in keys {
        let linked = removed.and_then(|r| r.iter().find(|(k, _)| k == key)).and_then(|(_, v)| v.as_str());
        let item = linked.and_then(|bt| baseline.and_then(|b| b.iter().find(|(k, _)| k == bt))).map(|(_, v)| v);
        let name = item.and_then(|i| i.get("ref")).and_then(|r| r.get("name")).and_then(Value::as_str);
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            names.push(name.to_string());
        }
    }
    let keys = keys.to_vec();
    let change: Change = Arc::new(move |state: &mut Map| remove_from(state, "local_removed", &keys));
    (change, names)
}

/// Storage amounts that differ: `mine` (this side stays) or `theirs` (the
/// Basetool's stays).
pub fn stock_decide(keys: Vec<String>, choice: &str) -> Change {
    let choice = choice.to_string();
    Arc::new(move |state: &mut Map| {
        let stock = object_mut(state, "stock");
        let decisions = object_mut(stock, "decisions");
        for key in &keys {
            map_set(decisions, key, Value::str(&choice));
        }
        remove_from(stock, "conflicts", &keys);
    })
}

/// Ships missing here: also remove them in the Basetool.
pub fn ships_release(exts: Vec<String>) -> Change {
    Arc::new(move |state: &mut Map| add_to(object_mut(state, "ships"), "pending_removals", &exts))
}

/// Ships missing here stay in the Basetool: the link goes and nobody asks
/// again. ⚠ The link is also at the server (`externalId`) and would come
/// back with the next fetch; the `kept` list stops the question.
pub fn ships_keep(exts: Vec<String>) -> Change {
    Arc::new(move |state: &mut Map| {
        let ships = object_mut(state, "ships");
        remove_from(ships, "local_removed", &exts);
        add_to(ships, "kept", &exts);
    })
}

/// Ships removed in the Basetool: create them again anyway.
pub fn ships_override(exts: Vec<String>) -> Change {
    Arc::new(move |state: &mut Map| add_to(object_mut(state, "ships"), "overrides", &exts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::loads;

    fn state(text: &str) -> Map {
        match loads(text).expect("json") {
            Value::Object(map) => map,
            other => panic!("kein Objekt: {other:?}"),
        }
    }

    fn texts(map: &Map, path: &[&str]) -> Vec<String> {
        let mut at = Value::Object(map.clone());
        for key in path {
            at = at.get(key).cloned().unwrap_or(Value::Null);
        }
        match at {
            Value::Array(items) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        }
    }

    fn keys(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    /// Sorted, without doubles, and the same decision twice changes nothing.
    #[test]
    fn a_release_adds_sorted_and_only_once() {
        let mut s = state(r#"{"pending_removals": ["b", "a"]}"#);
        let change = release_removals(keys(&["c", "a", "ä"]));
        change(&mut s);
        assert_eq!(texts(&s, &["pending_removals"]), ["a", "b", "c", "ä"]);
        let once = s.clone();
        change(&mut s);
        assert_eq!(s, once, "zweimal angewendet wie einmal");
        let mut empty = Map::new();
        override_conflicts(keys(&["x"]))(&mut empty);
        assert_eq!(texts(&empty, &["overrides"]), ["x"]);
    }

    /// Taking blueprints back: the names come from the last state of the
    /// Basetool; a key without a name only leaves the list.
    #[test]
    fn taking_back_names_what_comes_back() {
        let s = state(
            r#"{"local_removed": {"k1": "bt1", "k2": "bt2", "k3": "bt9"},
                "baseline": {"bt1": {"ref": {"name": "Pembroke Helm"}}, "bt2": {"ref": {}}}}"#,
        );
        let (change, names) = keep_removed(&s, &keys(&["k1", "k2", "k3", "k4"]));
        assert_eq!(names, ["Pembroke Helm"]);
        let mut after = s.clone();
        change(&mut after);
        let left = after.iter().find(|(k, _)| k == "local_removed").and_then(|(_, v)| v.as_object()).unwrap();
        assert!(left.is_empty(), "alle vier Schlüssel aus der Liste: {left:?}");
        assert_eq!(after.iter().find(|(k, _)| k == "baseline"), s.iter().find(|(k, _)| k == "baseline"));
    }

    /// Amounts: the choice is written per key, and the decided conflicts leave
    /// the list, the others stay.
    #[test]
    fn a_stock_decision_is_kept_and_closes_the_conflict() {
        let mut s = state(r#"{"stock": {"conflicts": {"a": [1, 2], "b": [3, 4]}, "decisions": {"z": "mine"}}}"#);
        stock_decide(keys(&["a"]), "theirs")(&mut s);
        let stock = s.iter().find(|(k, _)| k == "stock").map(|(_, v)| v.clone()).unwrap();
        assert_eq!(stock.get("decisions").and_then(|d| d.get("a")).and_then(Value::as_str), Some("theirs"));
        assert_eq!(stock.get("decisions").and_then(|d| d.get("z")).and_then(Value::as_str), Some("mine"));
        let conflicts = stock.get("conflicts").and_then(Value::as_object).unwrap();
        assert_eq!(conflicts.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(), ["b"]);
        // No stock yet: made on the way.
        let mut fresh = Map::new();
        stock_decide(keys(&["a"]), "mine")(&mut fresh);
        assert!(fresh.iter().any(|(k, _)| k == "stock"));
    }

    /// Ships: keeping them leaves the list and is remembered for good, the
    /// other two only add to their lists.
    #[test]
    fn ships_keep_release_and_override() {
        let mut s = state(r#"{"ships": {"local_removed": {"e1": "s1", "e2": "s2"}, "kept": ["e0"]}}"#);
        ships_keep(keys(&["e1"]))(&mut s);
        let ships = s.iter().find(|(k, _)| k == "ships").map(|(_, v)| v.clone()).unwrap();
        assert_eq!(ships.get("local_removed").and_then(Value::as_object).map(Vec::len), Some(1));
        let kept = ships.get("kept").cloned().unwrap();
        assert_eq!(kept, Value::Array(vec![Value::str("e0"), Value::str("e1")]));
        ships_release(keys(&["e2"]))(&mut s);
        ships_override(keys(&["e3"]))(&mut s);
        let ships = Value::Object(s.clone()).get("ships").cloned().unwrap();
        assert_eq!(ships.get("pending_removals"), Some(&Value::Array(vec![Value::str("e2")])));
        assert_eq!(ships.get("overrides"), Some(&Value::Array(vec![Value::str("e3")])));
    }
}
