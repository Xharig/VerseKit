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

//! The order of the sticks: which stick has which number (`js1`, `js2`, …), remembered, and put right on
//! request when the numbers were shuffled (USB port, restart, new driver).
//!
//! ## What is remembered, and what "shifted" means
//!
//! The player says once: "this is the order my binding is made for" ([`remember`]). The program writes down, per
//! stick, its id, its name and its number. Later it looks at the heads of the `actionmaps.xml`
//! (`<options type="joystick" instance="N" Product="Name {ID}">`), the one place where the game says which stick
//! has which number, and compares: **a stick whose number is another one than remembered is shifted** ([`plan`]).
//!
//! ⚠⚠ **The binding lines are what moves, never the heads.** The line `js1_button5` was made for the stick that had
//! number 1. If that stick has number 2 now, the line is put on `js2_button5` ([`rewrite_text`]); the head of the
//! stick, with its dead zones and curves, stays where it is: those belong to the physical stick. All lines of
//! all shifted sticks are renumbered **in one pass** (A to B and B to A at once), or the second step would
//! turn the first back.
//!
//! ⚠ **After a correction the remembered order is the new one** ([`correct`]): it is the order the binding lines
//! follow. Else the next look would find the sticks "shifted" again and turn the lines the wrong way.
//!
//! ## What is never guessed
//!
//! * **Two identical sticks** share one id; the file can not tell them apart. They are left alone and named
//!   ([`Plan::twins`]), like everywhere else on the page.
//! * **A stick that is not in the file** (unplugged, new id) is no reason to move anything ([`Plan::not_found`]).
//! * **A target number that already carries lines of something else** would mix two sticks: no change at all
//!   ([`Plan::conflict`], [`Failure::OrderConflict`]).
//!
//! Everything goes over [`crate::joysticks_write`] (text edits at the places, a copy first, never while the game
//! runs). The remembered order is an own file of the player (`joystick-reihenfolge.json`), no file of the game.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use regex::Regex;

use crate::game_files;
use crate::joysticks::{self, Assigned, actionmaps_path};
use crate::joysticks_write::{Done, Edit, Failure, apply, commit, open, parse};
use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::text::py_strip;

#[cfg(test)]
mod tests;

/// The file of the remembered order, among the own files of the program.
pub const FILE: &str = "joystick-reihenfolge.json";

/// One stick of the remembered order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// The id in capital letters.
    pub id: String,
    pub name: String,
    /// The number the binding lines were made for.
    pub number: i64,
}

/// The remembered order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remembered {
    /// `JJJJ-MM-TT HH:MM`, the time it was written down.
    pub stand: String,
    pub slots: Vec<Slot>,
}

/// One stick whose number is not the remembered one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shift {
    pub id: String,
    /// The name the head of the file gives it now.
    pub name: String,
    /// The remembered number: what the lines say now.
    pub from: i64,
    /// The number the head gives it now: what the lines are to say.
    pub to: i64,
    /// How many lines carry the number `from` now.
    pub bindings: usize,
}

/// What a look finds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub shifts: Vec<Shift>,
    /// Remembered sticks that stand in no head of the file now.
    pub not_found: Vec<Slot>,
    /// Remembered ids that two heads share (identical sticks): not touched.
    pub twins: Vec<Slot>,
    /// A number would be taken that carries lines of something else, or two heads share a number.
    pub conflict: bool,
}

impl Plan {
    /// Does anything have to be done?
    pub fn is_shifted(&self) -> bool {
        !self.shifts.is_empty()
    }

    /// How many lines are renumbered.
    pub fn lines(&self) -> usize {
        self.shifts.iter().map(|shift| shift.bindings).sum()
    }
}

/// The look of the page: what is remembered and what the file says to it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Check {
    pub remembered: Option<Remembered>,
    pub plan: Plan,
}

// ------------------------------------------------------------------ the memory

fn now_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_secs()).ok())
        .unwrap_or(0);
    crate::formats::local_time(seconds)
        .map(|time| crate::formats::strftime("%Y-%m-%d %H:%M", &time))
        .unwrap_or_default()
}

/// The remembered order; `None` where none was written down (or the file is no good).
pub fn remembered(paths: &Paths) -> Option<Remembered> {
    let Ok(Value::Object(data)) = jsonio::read_json(&paths.app_file(FILE)) else { return None };
    let value = Value::Object(data);
    let stand = value.get("stand").and_then(Value::as_str).unwrap_or("").to_string();
    let Some(Value::Array(list)) = value.get("geraete") else { return None };
    let slots: Vec<Slot> = list
        .iter()
        .filter_map(|entry| {
            let id = py_strip(entry.get("kennung").and_then(Value::as_str)?).to_uppercase();
            let number = match entry.get("nummer")? {
                Value::Int(text) => text.parse::<i64>().ok()?,
                _ => return None,
            };
            let name = entry.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            (!id.is_empty()).then_some(Slot { id, name, number })
        })
        .collect();
    (!slots.is_empty()).then_some(Remembered { stand, slots })
}

fn save(paths: &Paths, order: &Remembered) -> Result<(), Failure> {
    let list = order
        .slots
        .iter()
        .map(|slot| {
            Value::Object(vec![
                ("kennung".to_string(), Value::str(slot.id.clone())),
                ("name".to_string(), Value::str(slot.name.clone())),
                ("nummer".to_string(), Value::int(slot.number)),
            ])
        })
        .collect();
    let data = Value::Object(vec![
        ("stand".to_string(), Value::str(order.stand.clone())),
        ("geraete".to_string(), Value::Array(list)),
    ]);
    jsonio::save_json(&paths.app_file(FILE), &data, 1, false).map_err(|_| Failure::OrderWrite)
}

/// The sticks of a binding with exactly one head, and the ids that have more than one.
fn split_unique(stored: &[Assigned]) -> (Vec<&Assigned>, BTreeSet<String>) {
    let mut count: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in stored.iter().filter(|e| !e.id.is_empty()) {
        *count.entry(entry.id.as_str()).or_default() += 1;
    }
    let twins = count.iter().filter(|(_, n)| **n > 1).map(|(id, _)| (*id).to_string()).collect();
    let unique = stored.iter().filter(|e| !e.id.is_empty() && count.get(e.id.as_str()) == Some(&1)).collect();
    (unique, twins)
}

/// The remembered order of a binding text: every stick with one head of its own.
#[must_use]
pub fn capture_text(text: &str) -> Remembered {
    let stored = joysticks::assignment_from_text(text);
    let (unique, _) = split_unique(&stored);
    let slots =
        unique.iter().map(|entry| Slot { id: entry.id.clone(), name: entry.name.clone(), number: entry.number }).collect();
    Remembered { stand: now_stamp(), slots }
}

/// Writes down the order the file has now. Gives how many sticks went into it.
///
/// # Errors
///
/// [`Failure::NoFile`], [`Failure::Unreadable`], [`Failure::Nothing`] where no stick has a head of its own,
/// [`Failure::OrderWrite`].
pub fn remember(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Result<usize, Failure> {
    let read = open(paths, file, folder)?;
    parse(&read.text).ok_or(Failure::Unreadable)?;
    let order = capture_text(&read.text);
    if order.slots.is_empty() {
        return Err(Failure::Nothing);
    }
    save(paths, &order)?;
    Ok(order.slots.len())
}

// ------------------------------------------------------------------- the plan

fn line_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"(^|[^A-Za-z0-9_])js([0-9]+)_").expect("pattern"))
}

/// How many lines (attributes `input`) carry which stick number.
fn used_numbers(text: &str) -> Option<BTreeMap<i64, usize>> {
    let document = parse(text)?;
    let mut used: BTreeMap<i64, usize> = BTreeMap::new();
    for node in document.descendants().filter(roxmltree::Node::is_element) {
        let Some(value) = node.attribute("input") else { continue };
        let numbers: BTreeSet<i64> =
            line_pattern().captures_iter(value).filter_map(|hit| hit[2].parse::<i64>().ok()).collect();
        for number in numbers {
            *used.entry(number).or_default() += 1;
        }
    }
    Some(used)
}

/// What is to be put right, from the remembered order and the text of the binding. `None` where the text is not
/// well formed.
#[must_use]
pub fn plan_from_text(order: &Remembered, text: &str) -> Option<Plan> {
    let used = used_numbers(text)?;
    let stored = joysticks::assignment_from_text(text);
    let (unique, twin_ids) = split_unique(&stored);
    let mut plan = Plan::default();

    // Two different sticks under one number: nothing can be said.
    let mut seen: BTreeSet<i64> = BTreeSet::new();
    for entry in &stored {
        if !seen.insert(entry.number) {
            plan.conflict = true;
        }
    }
    // A remembered id twice (or with a number twice) is not clear either.
    let mut remembered_ids: BTreeSet<&str> = BTreeSet::new();
    for slot in &order.slots {
        if !remembered_ids.insert(slot.id.as_str()) {
            plan.conflict = true;
        }
    }

    for slot in &order.slots {
        if twin_ids.contains(&slot.id) {
            plan.twins.push(slot.clone());
            continue;
        }
        let Some(entry) = unique.iter().find(|e| e.id == slot.id) else {
            plan.not_found.push(slot.clone());
            continue;
        };
        if entry.number != slot.number {
            plan.shifts.push(Shift {
                id: slot.id.clone(),
                name: if entry.name.is_empty() { slot.name.clone() } else { entry.name.clone() },
                from: slot.number,
                to: entry.number,
                bindings: used.get(&slot.number).copied().unwrap_or(0),
            });
        }
    }

    let sources: BTreeSet<i64> = plan.shifts.iter().map(|s| s.from).collect();
    let targets: BTreeSet<i64> = plan.shifts.iter().map(|s| s.to).collect();
    if sources.len() != plan.shifts.len() || targets.len() != plan.shifts.len() {
        plan.conflict = true;
    }
    // A target that is no source and still carries lines would swallow the lines of something else.
    if targets.iter().any(|t| !sources.contains(t) && used.contains_key(t)) {
        plan.conflict = true;
    }
    Some(plan)
}

/// The text with the lines of the shifted sticks on their new numbers, in one pass. Gives the text and how many
/// lines were changed.
///
/// # Errors
///
/// [`Failure::Unreadable`] where the text is not well formed, [`Failure::Same`] where no line changes,
/// [`Failure::Broken`] where the result would not be well formed.
pub fn rewrite_text(text: &str, shifts: &[Shift]) -> Result<(String, usize), Failure> {
    let document = parse(text).ok_or(Failure::Unreadable)?;
    let map: BTreeMap<i64, i64> = shifts.iter().map(|s| (s.from, s.to)).collect();
    let mut edits = Vec::new();
    for node in document.descendants().filter(|n| n.is_element()) {
        let Some(attr) = node.attribute_node("input").filter(|a| a.name() == "input") else { continue };
        let raw = &text[attr.range_value()];
        let changed = line_pattern().replace_all(raw, |hit: &regex::Captures| {
            let number = hit[2].parse::<i64>().ok().and_then(|n| map.get(&n).copied());
            match number {
                Some(to) => format!("{}js{to}_", &hit[1]),
                None => hit[0].to_string(),
            }
        });
        if changed != raw {
            // The value as it stands between the quotes is kept as it is; only the digits changed.
            edits.push(Edit { range: attr.range_value(), with: changed.into_owned() });
        }
    }
    if edits.is_empty() {
        return Err(Failure::Same);
    }
    let count = edits.len();
    let new = apply(text, edits);
    if parse(&new).is_none() {
        return Err(Failure::Broken);
    }
    Ok((new, count))
}

/// The look at the file: what is remembered and what is shifted. Without a file or a memory the plan is empty.
pub fn check(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Check {
    let remembered = remembered(paths);
    let Some(order) = remembered.as_ref() else { return Check::default() };
    let Some(path) = file.map(str::to_string).or_else(|| actionmaps_path(paths, folder)) else {
        return Check { remembered, plan: Plan::default() };
    };
    let plan = joysticks::read_xml(&path).and_then(|text| plan_from_text(order, &text)).unwrap_or_default();
    Check { remembered, plan }
}

// ------------------------------------------------------------- the correction

/// What a correction did: enough to take it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Applied {
    pub done: Done,
    /// The file as written; the way back only goes while the file still is this.
    pub written: Option<String>,
    /// The memory before the correction.
    pub previous: Remembered,
    pub shifts: Vec<Shift>,
}

/// Puts the lines of the shifted sticks on their numbers, with a copy first and never while the game runs; then
/// the remembered order is the order of the file now.
///
/// # Errors
///
/// [`Failure::OrderNothing`] without a memory, [`Failure::Same`] where nothing is shifted,
/// [`Failure::OrderConflict`], and the refusals of the writing.
pub fn correct(paths: &Paths, file: Option<&str>, folder: Option<&str>) -> Result<Applied, Failure> {
    let order = remembered(paths).ok_or(Failure::OrderNothing)?;
    let read = open(paths, file, folder)?;
    let plan = plan_from_text(&order, &read.text).ok_or(Failure::Unreadable)?;
    if !plan.is_shifted() {
        return Err(Failure::Same);
    }
    if plan.conflict {
        return Err(Failure::OrderConflict);
    }
    let (new, count, written) = match rewrite_text(&read.text, &plan.shifts) {
        Ok((new, count)) => (Some(new), count, true),
        // Shifted sticks without a single line: only the memory has to follow.
        Err(Failure::Same) => (None, 0, false),
        Err(other) => return Err(other),
    };
    let done = match &new {
        Some(new) => commit(paths, &read, new, count)?,
        None => Done { backup: None, count: 0 },
    };
    let mut next = order.clone();
    next.stand = now_stamp();
    for shift in &plan.shifts {
        if let Some(slot) = next.slots.iter_mut().find(|s| s.id == shift.id) {
            slot.number = shift.to;
        }
    }
    if let Err(failure) = save(paths, &next) {
        // The file was changed and the memory could not follow: the next look would turn the lines back. So the
        // file goes back (the copy is there), and the player is told.
        if written {
            let _ = game_files::restore_newest(paths, &read.path);
        }
        return Err(failure);
    }
    Ok(Applied { done, written: new, previous: order, shifts: plan.shifts })
}

/// Takes a correction back: the copy comes back and the memory is the old one again. Only while the file still is
/// what the correction wrote.
///
/// # Errors
///
/// [`Failure::Changed`] where the file is another one now, [`Failure::NoFile`], and the refusals of the writing.
pub fn undo(paths: &Paths, applied: &Applied, file: Option<&str>, folder: Option<&str>) -> Result<(), Failure> {
    if let Some(written) = &applied.written {
        let read = open(paths, file, folder)?;
        if &read.text != written {
            return Err(Failure::Changed);
        }
        game_files::restore_newest(paths, &read.path).map_err(Failure::Write)?;
    } else {
        game_files::check_quiet(paths).map_err(Failure::Write)?;
    }
    save(paths, &applied.previous)
}
