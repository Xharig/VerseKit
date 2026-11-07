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

//! The running contracts of the overlay, like the contract part of the
//! `Watcher` of 4.12 (`_auftraege_beim_start`, `_auftraege_melden`,
//! `_unklar_tick`, `auftrag_wegklicken`) and `contract_hidden.py`.
//!
//! One state, no history: what is open right now, with the line that says how
//! many blueprints it gives, its open objectives and whether it fell silent
//! after a change of server. Everything the thread tells the window comes out
//! as [`Out`] in the order the original put it into its queue.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::contracts::{
    Event, Kind, MissionKeys, ObjectiveEvent, Objectives, Patterns, contracts_from_text, events_from_text,
    objective_events_from_text, server_change_silence, state_from_text, which_ended,
};
use crate::jsonio::{self, Newline, Value};
use crate::language::{Phrase, tf};
use crate::logsource::ContractScan;
use crate::mission_log::{ContractBlueprints, clean};
use crate::overlay::Message;
use crate::paths::Paths;

/// The contracts the player hid by hand, by mission id.
pub const HIDDEN_FILE: &str = "auftraege-ausgeblendet.json";

/// How many ids are kept at most, the newest last.
pub const HIDDEN_KEEP: usize = 300;

/// The mission ids hidden by hand.
pub fn hidden_load(paths: &Paths) -> HashSet<String> {
    let file = paths.app_file(HIDDEN_FILE);
    let valid = |v: &Value| matches!(v, Value::Object(_));
    let data = jsonio::load_json(&file, Some(&valid), "contract_hidden.load");
    ids_of(data.as_ref()).into_iter().filter(|i| !i.is_empty()).collect()
}

/// The entries of `ids` in a file as Python iterates them: a list gives its
/// texts, a text gives its single characters (a file edited by hand).
fn ids_of(data: Option<&Value>) -> Vec<String> {
    ids_values(data).into_iter().filter_map(|v| v.as_str().map(str::to_string)).collect()
}

/// The same, as the values stand there: a list gives its entries (also the ones that are no
/// text), a text its characters, an object its keys.
fn ids_values(data: Option<&Value>) -> Vec<Value> {
    match data.and_then(|d| d.get("ids")) {
        Some(Value::Array(items)) => items.clone(),
        Some(Value::Str(text)) => text.chars().map(|c| Value::Str(c.to_string())).collect(),
        Some(Value::Object(map)) => map.iter().map(|(k, _)| Value::Str(k.clone())).collect(),
        _ => Vec::new(),
    }
}

/// `json.dump` with the default `ensure_ascii`: everything outside ASCII as `\uXXXX`.
fn ascii_only(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// Takes mission ids into the list and saves it. True if that worked.
///
/// ⚠ What a hand-edited file holds besides texts stays in the list, as 4.12 writes it back.
pub fn hidden_add(paths: &Paths, ids: &[String]) -> bool {
    let new: Vec<&String> = ids.iter().filter(|i| !i.is_empty()).collect();
    if new.is_empty() {
        return false;
    }
    let file = paths.app_file(HIDDEN_FILE);
    let old = ids_values(jsonio::read_json(&file).ok().as_ref());
    let mut all: Vec<Value> =
        old.into_iter().filter(|v| !v.as_str().is_some_and(|s| new.iter().any(|n| n.as_str() == s))).collect();
    all.extend(new.into_iter().map(|i| Value::str(i.as_str())));
    let keep = all.len().saturating_sub(HIDDEN_KEEP);
    let data =
        Value::Object(vec![("version".into(), Value::int(1)), ("ids".into(), Value::Array(all.split_off(keep)))]);
    let text = ascii_only(&jsonio::dumps(&data, Some(1), false));
    match jsonio::write_text_atomic(&file, &text, Newline::Native) {
        Ok(()) => true,
        Err(e) => {
            crate::errors::record("contract_hidden.add", &e);
            false
        }
    }
}

/// Takes mission ids out of the list again (the player took the hiding back) and saves it.
/// True if they are out of the list afterwards, also when none of them was in it.
///
/// ⚠ Like [`hidden_add`] this keeps whatever else a hand-edited file holds.
pub fn hidden_remove(paths: &Paths, ids: &[String]) -> bool {
    let gone: Vec<&String> = ids.iter().filter(|i| !i.is_empty()).collect();
    if gone.is_empty() {
        return true;
    }
    let file = paths.app_file(HIDDEN_FILE);
    let Ok(data) = jsonio::read_json(&file) else { return true };
    let old = ids_values(Some(&data));
    let all: Vec<Value> =
        old.iter().filter(|v| !v.as_str().is_some_and(|s| gone.iter().any(|g| g.as_str() == s))).cloned().collect();
    if all.len() == old.len() {
        return true;
    }
    let data = Value::Object(vec![("version".into(), Value::int(1)), ("ids".into(), Value::Array(all))]);
    let text = ascii_only(&jsonio::dumps(&data, Some(1), false));
    match jsonio::write_text_atomic(&file, &text, Newline::Native) {
        Ok(()) => true,
        Err(e) => {
            crate::errors::record("contract_hidden.remove", &e);
            false
        }
    }
}

/// Are all these ids hidden? Without an id: no.
pub fn is_hidden(ids: &[String], known: &HashSet<String>) -> bool {
    let ids: Vec<&String> = ids.iter().filter(|i| !i.is_empty()).collect();
    !ids.is_empty() && ids.iter().all(|i| known.contains(*i))
}

/// What one section of the Game.log said about contracts.
#[derive(Clone, Debug, Default)]
pub struct Section {
    /// Accepted, ended and left the world, in the order of the log.
    pub events: Vec<Event>,
    /// The raw messages about objectives.
    pub objectives: Vec<ObjectiveEvent>,
    /// Mission id → the contract of the catalog behind it.
    pub markers: HashMap<String, String>,
}

/// Reads each new section for contract messages; the thread picks the result
/// up through [`SectionScan::taken`].
pub struct SectionScan {
    patterns: Patterns,
    section: Arc<Mutex<Section>>,
}

impl SectionScan {
    pub fn new(patterns: Patterns) -> (SectionScan, Arc<Mutex<Section>>) {
        let section = Arc::new(Mutex::new(Section::default()));
        (SectionScan { patterns, section: Arc::clone(&section) }, section)
    }

    /// The section read so far, taken out of the shared place.
    pub fn taken(section: &Mutex<Section>) -> Section {
        std::mem::take(&mut *section.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl ContractScan for SectionScan {
    fn clear(&mut self) {
        *self.section.lock().unwrap_or_else(|e| e.into_inner()) = Section::default();
    }

    fn scan(&mut self, text: &str) {
        let section = Section {
            events: events_from_text(text, &self.patterns),
            objectives: objective_events_from_text(text),
            markers: contracts_from_text(text),
        };
        *self.section.lock().unwrap_or_else(|e| e.into_inner()) = section;
    }
}

/// One running contract as the bar shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The contract (title without our marks).
    pub title: String,
    /// The line: perhaps with the blueprints, else only that it was accepted.
    pub text: Message,
    /// The open objectives, in the order of the log.
    pub goals: Vec<String>,
    /// Silent since a change of server: grey, with an explanation.
    pub grey: bool,
}

/// What the thread tells the window.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// The whole state of the bar, not the change: it cannot drift apart.
    Stand(Vec<Line>),
    /// The contract is gone: the hint rows that belong to it go too.
    Gone(String),
    /// A hint row that belongs to the contract.
    Hint { text: Message, contract: String },
}

/// What the check needs: the catalog, and whether a blueprint is already in the collection.
pub struct Check<'a> {
    pub blueprints: &'a ContractBlueprints,
    pub has: &'a dyn Fn(&str) -> bool,
}

/// The state of the running contracts.
#[derive(Default)]
pub struct Live {
    /// Seen in this run, against double messages.
    seen: HashSet<String>,
    /// What is running: title → line, in the order of the contracts.
    open: Vec<(String, Message)>,
    /// The only bridge when the end title does not fit the title of the acceptance.
    missions: MissionKeys,
    /// Title → the contract id from the log.
    contract_of: HashMap<String, String>,
    /// What a line was built with (contract id or none): a contract that
    /// arrives later has the line built again.
    line_source: HashMap<String, Option<String>>,
    /// Contracts that stayed silent after a change of server.
    unclear: HashSet<String>,
    /// Set when the game ends: the silent ones go.
    pub clear_unclear: bool,
    objectives: Objectives,
}

fn set_open(open: &mut Vec<(String, Message)>, title: &str, line: Message) {
    match open.iter_mut().find(|(t, _)| t == title) {
        Some(slot) => slot.1 = line,
        None => open.push((title.to_string(), line)),
    }
}

impl Live {
    pub fn new() -> Live {
        Live::default()
    }

    /// Is something running?
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// The titles of what is running, in order.
    pub fn titles(&self) -> Vec<String> {
        self.open.iter().map(|(t, _)| t.clone()).collect()
    }

    /// The one place that gives the state to the outside.
    pub fn stand(&self) -> Vec<Line> {
        let mut first: HashMap<&str, &str> = HashMap::new();
        for (id, title) in self.missions.iter() {
            first.entry(title.as_str()).or_insert(id.as_str());
        }
        self.open
            .iter()
            .map(|(title, text)| Line {
                title: title.clone(),
                text: text.clone(),
                goals: self.objectives.open_for(first.get(title.as_str()).copied()),
                grey: self.unclear.contains(title),
            })
            .collect()
    }

    fn ids_of(&self, title: &str) -> Vec<String> {
        self.missions.ids_of(title)
    }

    /// The line to a contract: with the blueprints if the catalog knows it.
    ///
    /// ⚠ Unknown: only the title is left. That is no promise, only the name the
    /// game writes into the log. A made-up blueprint figure would be worse than none.
    fn line_for(&self, check: &Check, title: &str, plain: &str) -> Option<Message> {
        let contract = self.contract_of.get(plain).map(String::as_str);
        let (total, missing) = check.blueprints.check(title, check.has, contract)?;
        // ⚠ The fraction is the answer, not the total: „23/54“ says whether
        // there is still something to get.
        let have = total - missing.len();
        let head = tf("auftrag_zeile", &[("0", plain.into())]);
        // ⚠ From two missing on no names are given: a selection without value.
        let tail = match missing.as_slice() {
            [] => tf("auftrag_komplett", &[("0", have.into()), ("1", total.into())]),
            [one] => tf("auftrag_fehlt", &[("0", have.into()), ("1", total.into()), ("2", one.as_str().into())]),
            _ => tf("auftrag_stand", &[("0", have.into()), ("1", total.into())]),
        };
        Some(Message::Plain(tf("ov_a_zeile_mit", &[("0", head.into()), ("1", tail.into())])))
    }

    fn bare_line(plain: &str) -> Message {
        Phrase::new("auftrag_zeile").with("0", plain).into()
    }

    /// What is open according to the running Game.log; the state only, no rows.
    pub fn at_start(
        &mut self,
        text: &str,
        patterns: &Patterns,
        check: &Check,
        hidden: &HashSet<String>,
        out: &mut Vec<Out>,
    ) {
        let (open, missions) = state_from_text(text, patterns);
        // The ids too: when a contract ends later, the mission id is often the only link.
        for (id, key) in missions.iter() {
            self.missions.insert(id, key);
        }
        let by_mission = contracts_from_text(text);
        for (id, key) in missions.iter() {
            if let Some(contract) = by_mission.get(id) {
                self.contract_of.insert(key.clone(), contract.clone());
            }
        }
        self.objectives.absorb(&objective_events_from_text(text));
        for title in &open {
            let plain = clean(title);
            if plain.is_empty() {
                continue;
            }
            let ids: Vec<String> = missions.ids_of(&plain);
            if is_hidden(&ids, hidden) {
                continue;
            }
            // At the start nothing goes into the list of rows: only the state is set.
            self.seen.insert(plain.clone());
            self.line_source.insert(plain.clone(), self.contract_of.get(&plain).cloned());
            let line = self.line_for(check, title, &plain).unwrap_or_else(|| Live::bare_line(&plain));
            set_open(&mut self.open, &plain, line);
        }
        self.silence(Some(text), "", out);
        if !self.open.is_empty() {
            out.push(Out::Stand(self.stand()));
        }
    }

    /// Marks contracts that stayed silent after a change of server and
    /// takes the mark away when one reports again. `text` is the text of the
    /// section when `None`.
    ///
    /// ⚠ A contract without mission id never turns grey.
    pub fn silence(&mut self, text: Option<&str>, section_text: &str, out: &mut Vec<Out>) -> bool {
        let mut changed = false;
        if self.clear_unclear {
            self.clear_unclear = false;
            let mut gone: Vec<String> = self.unclear.iter().cloned().collect();
            gone.sort();
            for title in gone {
                if let Some(at) = self.open.iter().position(|(t, _)| *t == title) {
                    self.open.remove(at);
                    out.push(Out::Gone(title));
                    changed = true;
                }
            }
            self.unclear.clear();
        }
        let text = text.unwrap_or(section_text);
        if !text.is_empty() && !self.open.is_empty() {
            let mut per_title: HashMap<String, Vec<String>> = HashMap::new();
            for (id, title) in self.missions.iter() {
                per_title.entry(title.clone()).or_default().push(id.clone());
            }
            let all: Vec<String> = per_title.values().flatten().cloned().collect();
            let (changed_server, silent, reported) = server_change_silence(text, &all);
            for (title, _) in &self.open {
                let ids = per_title.get(title).cloned().unwrap_or_default();
                if ids.is_empty() {
                    continue;
                }
                if changed_server && ids.iter().all(|i| silent.contains(i)) {
                    if self.unclear.insert(title.clone()) {
                        changed = true;
                    }
                } else if self.unclear.contains(title) && ids.iter().any(|i| reported.contains(i)) {
                    self.unclear.remove(title);
                    changed = true;
                }
            }
        }
        let open: HashSet<&String> = self.open.iter().map(|(t, _)| t).collect();
        self.unclear.retain(|t| open.contains(t));
        if changed {
            out.push(Out::Stand(self.stand()));
        }
        changed
    }

    /// Takes a contract out of the display by hand.
    ///
    /// ⚠ The game does not report every loss: a contract is gone through a
    /// fault of the game without an end in the log. The player knows. The
    /// title stays in `seen` so it does not come back with the next section;
    /// the mission ids go into the file so it stays away after a restart.
    pub fn hide(&mut self, paths: &Paths, title: &str, out: &mut Vec<Out>) {
        hidden_add(paths, &self.ids_of(title));
        self.unclear.remove(title);
        if let Some(at) = self.open.iter().position(|(t, _)| t == title) {
            self.open.remove(at);
            out.push(Out::Stand(self.stand()));
        }
        // Also when it was gone from the bar already: the hint row may still stand.
        out.push(Out::Gone(title.to_string()));
    }

    /// Takes the hiding back: the mission ids of the contract leave the file and the title may show again.
    /// The bar is built anew from the log by the caller (`reset`), which finds the contract if it still runs.
    /// False when the ids could not be taken out of the file.
    ///
    /// ⚠ Not in 4.12 (a hidden contract stayed hidden); asked for by the rules of the surface
    /// (a deletion is undone, not asked about).
    pub fn unhide(&mut self, paths: &Paths, title: &str) -> bool {
        let done = hidden_remove(paths, &self.ids_of(title));
        self.seen.remove(title);
        done
    }

    /// For each accepted contract say whether blueprints are in it, and take
    /// the ended ones away. `section_text` is the text of the new section.
    ///
    /// ⚠ Where the catalog does not know the contract, nothing is claimed.
    pub fn report(&mut self, paths: &Paths, section: &Section, section_text: &str, check: &Check, out: &mut Vec<Out>) {
        // First the objectives: they change without the list of contracts moving.
        let goals_new = self.objectives.absorb(&section.objectives);
        if section.events.is_empty() {
            if goals_new && !self.open.is_empty() {
                out.push(Out::Stand(self.stand()));
            }
            self.silence(None, section_text, out);
            return;
        }
        let mut changed = false;
        // ⚠ In the order of the log, not all ends first: what is open at the
        // end of the section is shown; what had an end last is not.
        let mut now_open: Vec<(String, String)> = Vec::new();
        for event in &section.events {
            if event.kind == Kind::Left {
                // Leaving the world clears everything, before any title is looked at.
                now_open.clear();
                self.unclear.clear();
                let all: Vec<String> = self.open.drain(..).map(|(t, _)| t).collect();
                for title in all {
                    out.push(Out::Gone(title.clone()));
                    changed = true;
                    // The same contract shows again when it is accepted again after the next login.
                    self.seen.remove(&title);
                }
                let ids: Vec<String> = self.missions.iter().map(|(i, _)| i.clone()).collect();
                for id in ids {
                    self.objectives.forget(&id);
                    self.missions.remove(&id);
                }
                continue;
            }
            let plain = clean(&event.title);
            if event.kind == Kind::Accept {
                // An acceptance without a title is worthless.
                if plain.is_empty() {
                    continue;
                }
                match now_open.iter_mut().find(|(p, _)| *p == plain) {
                    Some(slot) => slot.1 = event.title.clone(),
                    None => now_open.push((plain.clone(), event.title.clone())),
                }
                if !event.mission_id.is_empty() {
                    self.missions.insert(&event.mission_id, &plain);
                    if let Some(contract) = section.markers.get(&event.mission_id).filter(|c| !c.is_empty()) {
                        self.contract_of.insert(plain.clone(), contract.clone());
                    }
                }
                continue;
            }
            // An end may be without title, but not without id: nothing is guessed.
            if plain.is_empty() && event.mission_id.is_empty() {
                continue;
            }
            let known: Vec<&str> =
                self.open.iter().map(|(t, _)| t.as_str()).chain(now_open.iter().map(|(t, _)| t.as_str())).collect();
            let gone = which_ended(&plain, &event.mission_id, &event.objective_id, known, self.missions.as_map());
            let Some(gone) = gone else { continue };
            now_open.retain(|(p, _)| *p != gone);
            self.unclear.remove(&gone);
            if let Some(at) = self.open.iter().position(|(t, _)| *t == gone) {
                self.open.remove(at);
                changed = true;
            }
            for id in self.missions.remove_key(&gone) {
                self.objectives.forget(&id);
            }
            out.push(Out::Gone(gone.clone()));
            self.seen.remove(&gone);
        }

        let hidden = if now_open.is_empty() { HashSet::new() } else { hidden_load(paths) };
        for (plain, title) in &now_open {
            if is_hidden(&self.ids_of(plain), &hidden) {
                continue;
            }
            if !self.open.iter().any(|(t, _)| t == plain) {
                // The bare title is no promise; it may lead if the catalog does not know the mission.
                set_open(&mut self.open, plain, Live::bare_line(plain));
                changed = true;
            }
            // ⚠ A contract that arrives later has the line built again: the
            // shared message carries the null id and finds none, so the line
            // would stand on the rough figure for ever.
            let source = self.contract_of.get(plain).cloned();
            if self.seen.contains(plain) && self.line_source.get(plain).and_then(Clone::clone) == source {
                continue;
            }
            self.seen.insert(plain.clone());
            self.line_source.insert(plain.clone(), source);
            let Some(line) = self.line_for(check, title, plain) else { continue };
            set_open(&mut self.open, plain, line.clone());
            changed = true;
            // First the bar, then the hint: the other way round the window knows nothing yet.
            out.push(Out::Stand(self.stand()));
            out.push(Out::Hint { text: line, contract: plain.clone() });
        }
        if changed || goals_new {
            out.push(Out::Stand(self.stand()));
        }
        self.silence(None, section_text, out);
    }

    /// Everything once more from the log: the state is dropped and read again.
    /// Returns the titles that were running before.
    pub fn reset(&mut self) -> Vec<String> {
        let before = self.titles();
        self.open.clear();
        self.missions.clear();
        self.contract_of.clear();
        self.line_source.clear();
        self.unclear.clear();
        self.objectives = Objectives::default();
        before
    }

    /// After a new reading: what is no longer running is told, and an empty
    /// bar is told too, otherwise an old list would stand.
    pub fn after_reset(&self, before: &[String], out: &mut Vec<Out>) {
        for title in before {
            if !self.open.iter().any(|(t, _)| t == title) {
                out.push(Out::Gone(title.clone()));
            }
        }
        if self.open.is_empty() {
            out.push(Out::Stand(Vec::new()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::{Choice, set_language};

    fn patterns() -> Patterns {
        Patterns::of(&crate::contracts::start_phrases(&[]), &crate::contracts::end_phrases(&[]))
    }

    fn line(phrase: &str, title: &str, mission: &str, objective: &str) -> String {
        format!(
            "<2026-10-01T10:00:00.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{phrase}: {title}: \" [1] MissionId: [{mission}], ObjectiveId: [{objective}]\n"
        )
    }

    fn blueprints() -> ContractBlueprints {
        let mut missions = HashMap::new();
        missions.insert("Key_One".to_string(), vec!["A".to_string(), "B".to_string(), "C".to_string()]);
        // One folder for each call: the tests run side by side.
        static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("versekit-live-ini-{}-{n}", crate::testdir::run_tag()));
        std::fs::create_dir_all(&dir).expect("Ordner");
        let ini = dir.join("global.ini");
        std::fs::write(&ini, "Key_One=Hunt the Target\n").expect("ini");
        let known: HashSet<String> = missions.keys().cloned().collect();
        let index = crate::mission_log::ContractIndex::build(std::slice::from_ref(&ini), &known);
        let _ = std::fs::remove_dir_all(&dir);
        ContractBlueprints { index, missions, definitions: HashMap::new() }
    }

    fn paths(name: &str) -> (Paths, std::path::PathBuf) {
        use crate::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-live-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("Ordner");
        let home = dir.to_string_lossy().into_owned();
        let host = crate::paths::Host::new(Flavor::native(), &[(crate::legacy::ENV_HOME, home.as_str())], &home);
        (Paths::new(host), dir)
    }

    fn section_of(text: &str) -> Section {
        let (mut scan, shared) = SectionScan::new(patterns());
        scan.scan(text);
        SectionScan::taken(&shared)
    }

    #[test]
    fn an_accepted_contract_gets_its_line_and_goes_with_its_end() {
        set_language(Choice::De);
        let (paths, dir) = paths("annahme");
        let catalog = blueprints();
        let have = |n: &str| n == "A";
        let check = Check { blueprints: &catalog, has: &have };
        let mut live = Live::new();
        let text = line("Auftrag angenommen", "Hunt the Target", "m1", "");
        let mut out = Vec::new();
        live.report(&paths, &section_of(&text), &text, &check, &mut out);
        let Out::Stand(lines) = &out[0] else { panic!("{out:?}") };
        assert_eq!(lines[0].text.render(), "Auftrag angenommen: Hunt the Target  →  Baupläne 1/3");
        assert!(matches!(&out[1], Out::Hint { contract, .. } if contract == "Hunt the Target"));
        // The same message again says nothing.
        let mut out = Vec::new();
        live.report(&paths, &section_of(&text), &text, &check, &mut out);
        assert!(out.is_empty(), "{out:?}");
        // The end takes it away and tells the rows.
        let end = line("Auftrag abgeschlossen", "Hunt the Target", "m1", "");
        let mut out = Vec::new();
        live.report(&paths, &section_of(&end), &end, &check, &mut out);
        assert_eq!(out[0], Out::Gone("Hunt the Target".into()));
        assert!(live.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unknown_contract_stays_a_bare_title() {
        set_language(Choice::De);
        let (paths, dir) = paths("unbekannt");
        let catalog = blueprints();
        let have = |_: &str| false;
        let check = Check { blueprints: &catalog, has: &have };
        let mut live = Live::new();
        let text = line("Auftrag angenommen", "Something Else", "m2", "");
        let mut out = Vec::new();
        live.report(&paths, &section_of(&text), &text, &check, &mut out);
        let Some(Out::Stand(lines)) = out.last() else { panic!("{out:?}") };
        assert_eq!(lines[0].text.render(), "Auftrag angenommen: Something Else");
        assert!(!out.iter().any(|o| matches!(o, Out::Hint { .. })), "no hint for what the catalog does not know");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_silent_end_leaving_the_world_and_hiding_by_hand() {
        set_language(Choice::De);
        let (paths, dir) = paths("weg");
        let catalog = blueprints();
        let have = |_: &str| false;
        let check = Check { blueprints: &catalog, has: &have };
        let mut live = Live::new();
        let text = line("Auftrag angenommen", "Hunt the Target", "m1", "");
        live.report(&paths, &section_of(&text), &text, &check, &mut Vec::new());
        // Hidden by hand: out of the bar, the id in the file, gone for the rows.
        let mut out = Vec::new();
        live.hide(&paths, "Hunt the Target", &mut out);
        assert_eq!(out, [Out::Stand(Vec::new()), Out::Gone("Hunt the Target".into())]);
        assert!(hidden_load(&paths).contains("m1"));
        // Accepted again with the same id: it stays away.
        let mut other = Live::new();
        let mut out = Vec::new();
        other.report(&paths, &section_of(&text), &text, &check, &mut out);
        assert!(other.is_empty(), "{out:?}");
        // Leaving the world clears what is open.
        let again = line("Auftrag angenommen", "Hunt the Target", "m9", "");
        let mut third = Live::new();
        third.report(&paths, &section_of(&again), &again, &check, &mut Vec::new());
        let left = "<2026-10-01T10:00:01.000Z> [CSessionManager::RequestFrontEnd] Started - Reason\n";
        let mut out = Vec::new();
        third.report(&paths, &section_of(left), left, &check, &mut out);
        assert_eq!(out[0], Out::Gone("Hunt the Target".into()));
        assert!(third.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn taking_the_hiding_back_lets_the_contract_show_again() {
        set_language(Choice::De);
        let (paths, dir) = paths("zurueck");
        let catalog = blueprints();
        let have = |_: &str| false;
        let check = Check { blueprints: &catalog, has: &have };
        let mut live = Live::new();
        let text = line("Auftrag angenommen", "Hunt the Target", "m1", "");
        live.report(&paths, &section_of(&text), &text, &check, &mut Vec::new());
        live.hide(&paths, "Hunt the Target", &mut Vec::new());
        assert!(hidden_load(&paths).contains("m1"));
        // Taken back: the id is out of the file, and a contract accepted with that id shows again.
        assert!(live.unhide(&paths, "Hunt the Target"));
        assert!(!hidden_load(&paths).contains("m1"), "the id is out of the file");
        let mut other = Live::new();
        other.report(&paths, &section_of(&text), &text, &check, &mut Vec::new());
        assert!(!other.is_empty(), "the contract shows again");
        // A title that was never hidden: nothing to do, nothing broken.
        assert!(live.unhide(&paths, "Unknown"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn taking_ids_out_keeps_foreign_entries_and_the_others() {
        let (paths, dir) = paths("fremd-raus");
        let file = paths.app_file(HIDDEN_FILE);
        std::fs::create_dir_all(std::path::Path::new(&file).parent().expect("Ordner")).expect("anlegen");
        std::fs::write(&file, "{\"version\": 1, \"ids\": [\"alt\", 7, \"m1\", \"m2\"]}").expect("schreiben");
        assert!(hidden_remove(&paths, &["m1".to_string(), String::new()]));
        let text = std::fs::read_to_string(&file).expect("lesen");
        assert!(!text.contains("m1"), "{text}");
        assert!(text.contains("\"alt\"") && text.contains('7') && text.contains("\"m2\""), "{text}");
        // Without a file there is nothing to take out, and none is made.
        let _ = std::fs::remove_file(&file);
        assert!(hidden_remove(&paths, &["m1".to_string()]));
        assert!(!std::path::Path::new(&file).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn hiding_keeps_foreign_entries_and_writes_ascii_only() {
        let (paths, dir) = paths("fremd");
        let file = paths.app_file(HIDDEN_FILE);
        std::fs::create_dir_all(std::path::Path::new(&file).parent().expect("Ordner")).expect("anlegen");
        std::fs::write(&file, "{\"version\": 1, \"ids\": [\"alt\", 7]}").expect("schreiben");
        assert!(hidden_add(&paths, &["neu-\u{e4}".to_string()]));
        let text = std::fs::read_to_string(&file).expect("lesen");
        assert!(text.is_ascii(), "the file holds non-ASCII: {text}");
        assert!(text.contains("neu-\\u00e4"), "{text}");
        assert!(text.contains("\"alt\"") && text.contains('7'), "foreign entries lost: {text}");
        let known = hidden_load(&paths);
        assert!(known.contains("alt") && known.contains("neu-\u{e4}"), "{known:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn objectives_and_the_silence_after_a_change_of_server() {
        set_language(Choice::De);
        let (paths, dir) = paths("ziele");
        let catalog = blueprints();
        let have = |_: &str| false;
        let check = Check { blueprints: &catalog, has: &have };
        let mut live = Live::new();
        let accept = line("Auftrag angenommen", "Hunt the Target", "m1", "");
        live.report(&paths, &section_of(&accept), &accept, &check, &mut Vec::new());
        let goal = "<2026-10-01T10:00:00.000Z> <ObjectiveUpserted> x mission_id m1 - objective_id 0123abcd-1 - state MISSION_OBJECTIVE_STATE_INPROGRESS - flags=ShowInLog y\n\
                    <2026-10-01T10:00:01.000Z> Added notification \"Neues Ziel: Fliege los: \" [1] MissionId: [m1], ObjectiveId: [0123abcd-1]\n";
        let mut out = Vec::new();
        live.report(&paths, &section_of(goal), goal, &check, &mut out);
        let Some(Out::Stand(lines)) = out.last() else { panic!("{out:?}") };
        assert_eq!(lines[0].goals, ["Fliege los"]);
        // A change of server and nothing from m1 afterwards: grey.
        let change = "<2026-10-01T10:05:00.000Z> <Change Server Start>\n<2026-10-01T10:05:01.000Z> something else\n";
        let mut out = Vec::new();
        live.report(&paths, &section_of(change), change, &check, &mut out);
        let Some(Out::Stand(lines)) = out.last() else { panic!("{out:?}") };
        assert!(lines[0].grey);
        // It reports again: normal.
        let back = "<2026-10-01T10:06:00.000Z> blah m1 blah\n";
        let mut out = Vec::new();
        live.report(&paths, &section_of(back), back, &check, &mut out);
        let Some(Out::Stand(lines)) = out.last() else { panic!("{out:?}") };
        assert!(!lines[0].grey);
        // The game ends: the grey ones go.
        let mut out = Vec::new();
        live.report(&paths, &section_of(change), change, &check, &mut out);
        live.clear_unclear = true;
        let mut out = Vec::new();
        live.report(&paths, &section_of(""), "", &check, &mut out);
        assert!(live.is_empty(), "{out:?}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
