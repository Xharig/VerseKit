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

//! What the menu of the main window remembers: which page of each group was opened last and how often, so that a
//! click on the head of a group can open the page the player most likely wants, and the page the window starts
//! with. Pages are named by their text keys (`hf_liste`), groups by their ids (`bauplaene`).
//! Both settings are new in v5 and have keys of their own; 4.12 ignores them.

use crate::jsonio::{Map, Value};
use crate::paths::Paths;

/// The page opened last in each group: `{group id: page key}`.
pub const LAST_SETTING: &str = "bereich_zuletzt";
/// How often each page was opened, per group: `{group id: {page key: number}}`.
pub const COUNT_SETTING: &str = "bereich_zaehler";
/// The page the window starts with, as a page key; absent or empty is the overview.
pub const START_SETTING: &str = "startseite";

/// The most a counter keeps: a page opened more often than this does not win by more.
const MAX_COUNT: i64 = 1_000_000;

/// The pages opened per group, as the settings keep them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Visits {
    last: Vec<(String, String)>,
    counts: Vec<(String, Vec<(String, i64)>)>,
}

impl Visits {
    /// Reads both settings; a value that is not of the form above is left out.
    pub fn load(paths: &Paths) -> Visits {
        let settings = paths.settings();
        let object = |key: &str| match settings.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
            Some(Value::Object(map)) => map.clone(),
            _ => Map::new(),
        };
        let last = object(LAST_SETTING)
            .into_iter()
            .filter_map(|(group, page)| Some((group, page.as_str().filter(|p| !p.is_empty())?.to_string())))
            .collect();
        let counts = object(COUNT_SETTING)
            .into_iter()
            .filter_map(|(group, pages)| {
                let Value::Object(pages) = pages else { return None };
                let pages = pages
                    .into_iter()
                    .filter_map(|(page, n)| match n {
                        Value::Int(n) => Some((page, n.parse::<i64>().ok()?.clamp(0, MAX_COUNT))),
                        _ => None,
                    })
                    .collect();
                Some((group, pages))
            })
            .collect();
        Visits { last, counts }
    }

    /// A page of `group` was opened: it is the last one and counts once more.
    pub fn note(&mut self, group: &str, page: &str) {
        match self.last.iter_mut().find(|(g, _)| g == group) {
            Some((_, last)) => page.clone_into(last),
            None => self.last.push((group.to_string(), page.to_string())),
        }
        let at = match self.counts.iter().position(|(g, _)| g == group) {
            Some(at) => at,
            None => {
                self.counts.push((group.to_string(), Vec::new()));
                self.counts.len() - 1
            }
        };
        let pages = &mut self.counts[at].1;
        match pages.iter_mut().find(|(p, _)| p == page) {
            Some((_, n)) => *n = (*n + 1).min(MAX_COUNT),
            None => pages.push((page.to_string(), 1)),
        }
    }

    /// The page a click on the head of `group` opens, out of its `pages` as they are shown now: the one opened last,
    /// else the one opened most (the first of equals), else the first. `None` for a group without pages.
    pub fn target<'a>(&self, group: &str, pages: &[&'a str]) -> Option<&'a str> {
        let last = self.last.iter().find(|(g, _)| g == group).map(|(_, page)| page.as_str());
        if let Some(page) = pages.iter().find(|page| Some(**page) == last) {
            return Some(page);
        }
        let counts = self.counts.iter().find(|(g, _)| g == group).map(|(_, pages)| pages.as_slice()).unwrap_or(&[]);
        let count = |page: &str| counts.iter().find(|(p, _)| p == page).map_or(0, |(_, n)| *n);
        let mut best: Option<(&'a str, i64)> = None;
        for page in pages {
            let n = count(page);
            if n > 0 && best.is_none_or(|(_, most)| n > most) {
                best = Some((page, n));
            }
        }
        best.map(|(page, _)| page).or_else(|| pages.first().copied())
    }

    /// The two settings as they are written.
    pub fn values(&self) -> [(&'static str, Value); 2] {
        let last = self.last.iter().map(|(group, page)| (group.clone(), Value::str(page))).collect();
        let counts = self
            .counts
            .iter()
            .map(|(group, pages)| {
                let pages = pages.iter().map(|(page, n)| (page.clone(), Value::int(*n))).collect();
                (group.clone(), Value::Object(pages))
            })
            .collect();
        [(LAST_SETTING, Value::Object(last)), (COUNT_SETTING, Value::Object(counts))]
    }
}

/// The page key the window starts with; empty for the overview.
pub fn start_page(paths: &Paths) -> String {
    match paths.settings().iter().find(|(k, _)| k == START_SETTING).map(|(_, v)| v) {
        Some(Value::Str(page)) => crate::text::py_strip(page).to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests;
