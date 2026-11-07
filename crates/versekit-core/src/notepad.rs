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

//! The farm notes: single items one wants to craft or buy, without a ship
//! (`fleet.notepad*` of 4.12). They live in the hangar file under
//! `merkzettel`: `{name, ref, anzahl, weg}`.
//!
//! ⚠ Stored keys stay German. An item that is already on the notes gets its
//! count raised, never a second line. A count of zero or less removes it.

use crate::hangar::{field_text, slim};
use crate::jsonio::{Map, Value, map_set};
use crate::stock_data::py_float_value;
use crate::text::py_strip;

const KEY: &str = "merkzettel";

/// One item on the farm notes.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub name: String,
    /// The identifier of the recipe data, empty where there is none.
    pub reference: String,
    pub count: i64,
    /// `bauen` or `kaufen`; empty where the entry says none (counts as `bauen`).
    pub way: String,
    /// Ticked off: crafted and fitted.
    pub done: bool,
}

fn list(data: &Map) -> &[Value] {
    match data.iter().find(|(k, _)| k == KEY) {
        Some((_, Value::Array(items))) => items,
        _ => &[],
    }
}

fn list_mut(data: &mut Map) -> &mut Vec<Value> {
    if !matches!(data.iter().find(|(k, _)| k == KEY), Some((_, Value::Array(_)))) {
        map_set(data, KEY, Value::Array(Vec::new()));
    }
    match data.iter_mut().find(|(k, _)| k == KEY) {
        Some((_, Value::Array(items))) => items,
        _ => unreachable!("the list was made above"),
    }
}

/// `int(entry.get('anzahl') or 1)`.
fn count_of(entry: &Map) -> i64 {
    match entry.iter().find(|(k, _)| k == "anzahl").map(|(_, v)| v) {
        Some(v) if crate::stock_data::truthy(Some(v)) => py_float_value(v).map_or(1, |x| x.trunc() as i64),
        _ => 1,
    }
}

/// The items, alphabetical without case; an identifier with white space or
/// quotes is no identifier but a name and is dropped (`fleet.notepad`).
pub fn notes(data: &Map) -> Vec<Note> {
    let mut found: Vec<Note> = list(data)
        .iter()
        .filter_map(Value::as_object)
        .filter(|m| !field_text(m, "name").is_empty())
        .map(|m| {
            let reference = py_strip(field_text(m, "ref")).to_string();
            let bad = reference.chars().any(char::is_whitespace) || reference.contains('"');
            Note {
                name: field_text(m, "name").to_string(),
                reference: if bad { String::new() } else { reference },
                count: count_of(m),
                way: field_text(m, "weg").to_string(),
                done: crate::hangar::field_true(m, "erledigt"),
            }
        })
        .collect();
    found.sort_by_cached_key(|n| crate::stock_data::lower(&n.name));
    found
}

fn matching(entry: &Value, wanted: &str) -> bool {
    entry.as_object().is_some_and(|m| slim(field_text(m, "name")) == wanted)
}

/// Whether the item is on the notes.
pub fn contains(data: &Map, name: &str) -> bool {
    let wanted = slim(name);
    list(data).iter().any(|m| matching(m, &wanted))
}

/// Puts an item on the notes; true when it was new. An item that is there
/// gets `count` more.
pub fn add(data: &mut Map, name: &str, reference: &str, count: i64) -> bool {
    let name = py_strip(name);
    if name.is_empty() {
        return false;
    }
    let count = count.max(1);
    let wanted = slim(name);
    let items = list_mut(data);
    if let Some(Value::Object(entry)) = items.iter_mut().find(|m| matching(m, &wanted)) {
        let raised = count_of(entry) + count;
        map_set(entry, "anzahl", Value::int(raised));
        return false;
    }
    items.push(Value::Object(vec![
        ("name".into(), Value::str(name)),
        ("ref".into(), Value::str(py_strip(reference))),
        ("anzahl".into(), Value::int(count)),
        ("weg".into(), Value::str("bauen")),
    ]));
    true
}

/// Puts a struck item back exactly as it was (way, count, ticked off). False
/// where an item of that name is there again: it stays as it is.
pub fn restore(data: &mut Map, note: &Note) -> bool {
    let wanted = slim(&note.name);
    let items = list_mut(data);
    if items.iter().any(|m| matching(m, &wanted)) {
        return false;
    }
    let mut entry = vec![
        ("name".to_string(), Value::str(py_strip(&note.name))),
        ("ref".to_string(), Value::str(py_strip(&note.reference))),
        ("anzahl".to_string(), Value::int(note.count.max(1))),
        ("weg".to_string(), Value::str(if note.way.is_empty() { "bauen" } else { note.way.as_str() })),
    ];
    if note.done {
        entry.push(("erledigt".to_string(), Value::Bool(true)));
    }
    items.push(Value::Object(entry));
    true
}

/// Strikes an item; true when one fell away.
pub fn remove(data: &mut Map, name: &str) -> bool {
    let wanted = slim(name);
    let items = list_mut(data);
    let before = items.len();
    items.retain(|m| !matching(m, &wanted));
    items.len() != before
}

/// Sets how often the item is to be crafted; `0` or less strikes it. False
/// where the item is not on the notes.
pub fn set_count(data: &mut Map, name: &str, count: i64) -> bool {
    if count <= 0 {
        return remove(data, name);
    }
    let wanted = slim(name);
    match list_mut(data).iter_mut().find(|m| matching(m, &wanted)) {
        Some(Value::Object(entry)) => {
            map_set(entry, "anzahl", Value::int(count));
            true
        }
        _ => false,
    }
}

/// `count` pieces were crafted: the count falls, at zero the item goes.
/// True when the item was on the notes.
pub fn crafted(data: &mut Map, name: &str, count: i64) -> bool {
    let count = count.max(1);
    let wanted = slim(name);
    let Some(entry) = list(data).iter().find(|m| matching(m, &wanted)).and_then(Value::as_object) else {
        return false;
    };
    let (name, left) = (field_text(entry, "name").to_string(), count_of(entry) - count);
    set_count(data, &name, left);
    true
}

/// The count of an item on the notes, 0 where it is not there.
pub fn count_of_item(data: &Map, name: &str) -> i64 {
    let wanted = slim(name);
    list(data).iter().find(|m| matching(m, &wanted)).and_then(Value::as_object).map_or(0, count_of)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_that_is_there_gets_its_count_raised_and_no_second_line() {
        let mut data = Map::new();
        assert!(add(&mut data, "  Arctic-Storm Helm ", "", 2));
        assert!(!add(&mut data, "arctic storm helm", "", 3));
        assert_eq!(notes(&data).len(), 1);
        assert_eq!(notes(&data)[0].count, 5);
        assert!(contains(&data, "ARCTIC STORM HELM"));
    }

    #[test]
    fn crafting_lowers_the_count_and_zero_strikes_the_item() {
        let mut data = Map::new();
        add(&mut data, "Helm", "", 3);
        assert!(crafted(&mut data, "Helm", 2));
        assert_eq!(count_of_item(&data, "Helm"), 1);
        assert!(crafted(&mut data, "Helm", 5));
        assert!(!contains(&data, "Helm"));
        assert!(!crafted(&mut data, "Helm", 1));
    }

    #[test]
    fn an_identifier_with_white_space_is_dropped_when_reading() {
        let mut data = Map::new();
        add(&mut data, "CF-447 Rhino Repeater", "CF-447 Rhino Repeater", 1);
        add(&mut data, "Helm", "9fe902be-8b39-4d71-9017-e2fed7b0604c", 1);
        let found = notes(&data);
        assert_eq!(found[0].name, "CF-447 Rhino Repeater");
        assert_eq!(found[0].reference, "");
        assert_eq!(found[1].reference, "9fe902be-8b39-4d71-9017-e2fed7b0604c");
    }

    #[test]
    fn a_struck_item_comes_back_exactly_as_it_was() {
        let mut data = Map::new();
        add(&mut data, "Helm", "9fe902be-8b39-4d71-9017-e2fed7b0604c", 3);
        // Bought and ticked off: not the default of a new item.
        if let Some(Value::Array(items)) = data.iter_mut().find(|(k, _)| k == KEY).map(|(_, v)| v)
            && let Some(Value::Object(entry)) = items.first_mut()
        {
            map_set(entry, "weg", Value::str("kaufen"));
            map_set(entry, "erledigt", Value::Bool(true));
        }
        let before = notes(&data).remove(0);
        assert_eq!((before.way.as_str(), before.done), ("kaufen", true));
        assert!(remove(&mut data, "Helm"));
        assert!(restore(&mut data, &before));
        assert_eq!(notes(&data), vec![before.clone()]);
        // An item of that name that is there again stays as it is.
        assert!(!restore(&mut data, &Note { count: 9, ..before.clone() }));
        assert_eq!(notes(&data), vec![before]);
    }

    #[test]
    fn setting_the_count_to_zero_removes_and_an_unknown_item_is_refused() {
        let mut data = Map::new();
        add(&mut data, "Helm", "", 2);
        assert!(!set_count(&mut data, "Beine", 4));
        assert!(set_count(&mut data, "Helm", 0));
        assert!(notes(&data).is_empty());
    }
}
