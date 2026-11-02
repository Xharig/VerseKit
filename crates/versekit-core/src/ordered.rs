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

//! A map that keeps the order of first insertion, like Python's `dict`, and
//! the small helper that spreads work over the files of a folder.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Keys in the order they were first set; setting a key again keeps its place.
#[derive(Clone, Debug)]
pub struct OrderedMap<V> {
    entries: Vec<(String, V)>,
    index: HashMap<String, usize>,
}

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        OrderedMap { entries: Vec::new(), index: HashMap::new() }
    }
}

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    /// `dict[key] = value`.
    pub fn set(&mut self, key: &str, value: V) {
        match self.index.get(key) {
            Some(&i) => self.entries[i].1 = value,
            None => {
                self.index.insert(key.to_string(), self.entries.len());
                self.entries.push((key.to_string(), value));
            }
        }
    }

    /// `dict.setdefault(key, V())` and the value behind it.
    pub fn entry_or_default(&mut self, key: &str) -> &mut V
    where
        V: Default,
    {
        let i = match self.index.get(key) {
            Some(&i) => i,
            None => {
                self.index.insert(key.to_string(), self.entries.len());
                self.entries.push((key.to_string(), V::default()));
                self.entries.len() - 1
            }
        };
        &mut self.entries[i].1
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }
}

/// `work` on every item, spread over a few threads; the results come back in
/// the order of the items.
pub fn parallel_map<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8).min(items.len());
    if threads <= 1 {
        return items.iter().map(work).collect();
    }
    let next = AtomicUsize::new(0);
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let result = work(item);
                    *slots[i].lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                }
            });
        }
    });
    slots
        .into_iter()
        .map(|slot| slot.into_inner().unwrap_or_else(|e| e.into_inner()).expect("jede Arbeit getan"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_first_place_of_a_key() {
        let mut map: OrderedMap<i32> = OrderedMap::new();
        map.set("b", 1);
        map.set("a", 2);
        map.set("b", 3);
        assert_eq!(map.iter().collect::<Vec<_>>(), [("b", &3), ("a", &2)]);
        *map.entry_or_default("c") += 5;
        *map.entry_or_default("a") += 5;
        assert_eq!(map.values().copied().collect::<Vec<_>>(), [3, 7, 5]);
        assert_eq!(map.get("zz"), None);
    }

    #[test]
    fn parallel_results_keep_their_order() {
        let items: Vec<u64> = (0..200).collect();
        assert_eq!(parallel_map(&items, |n| n * 2), items.iter().map(|n| n * 2).collect::<Vec<_>>());
        assert_eq!(parallel_map(&[] as &[u64], |n| *n), Vec::<u64>::new());
        assert_eq!(parallel_map(&[7u64], |n| *n), [7]);
    }
}
