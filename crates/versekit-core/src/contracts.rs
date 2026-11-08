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

//! The contract messages of the Game.log: which phrases announce a contract,
//! how it ends, and what the lines around them say (mission and objective
//! ids, the end of a whole mission, leaving the game world, objective states).
//! Like `contracts.py`: the reading half, the bookkeeping of what is open
//! (`state_from_text`, `Objectives`) and the change of server; the check
//! against the catalog is `mission_log::ContractBlueprints::check`.
//!
//! ⚠ The phrases come from the player's own `global.ini`, not from here: the
//! table is only the fallback when no language file can be read.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;

use crate::mission_log::{PLACEHOLDER, clean};
use crate::text::{PY_NONSPACE, PY_SPACE, decode_ignore, lines_universal, py_strip};

/// The language-neutral key of a contract being accepted, and sharing it in the group.
///
/// ⚠ A shared offer is not an acceptance: it carries the null id, and most
/// are never accepted. `events_from_text` drops it.
pub const INI_KEYS: [&str; 2] = ["mobiGlas_ui_MissionEvent_Activated", "mobiGlas_ui_Mission_Shared"];

/// The three ways a contract ends: completed, withdrawn, failed.
pub const INI_END_KEYS: [&str; 3] =
    ["mobiGlas_ui_MissionEvent_Complete", "mobiGlas_ui_MissionEvent_Deactivate", "mobiGlas_ui_MissionEvent_Fail"];

/// Fallback phrases when the `global.ini` is missing, in the order German, English.
///
/// ⚠ Swiss German is a version of its own and writes `Uftrag` for `Auftrag`;
/// without these entries not one contract is recognised there.
const START_TABLE: [&[&str]; 2] = [
    &["Auftrag angenommen", "Auftrag geteilt", "Uftrag angenommen", "Uftrag geteilt"],
    &["Contract Accepted", "Contract Shared"],
];

/// ⚠ `Auftrag geteilt` does not belong here: it is a start, not an end.
const END_TABLE: [&[&str]; 2] = [
    &[
        "Auftrag abgeschlossen",
        "Auftrag zurückgezogen",
        "Auftrag fehlgeschlagen",
        "Uftrag abgschlosse",
        "Uftrag zurückgezogen",
        "Uftrag fehlgschlage",
    ],
    &["Contract Complete", "Contract Withdrawn", "Contract Failed"],
];

/// The id that means no id: every shared offer carries it.
pub const NULL_ID: &str = "00000000-0000-0000-0000-000000000000";

/// The line part every notification regular expression starts with.
const NOTIFICATION: &str = "Added notification \"";

/// The wording in the `global.ini` has a placeholder: `Auftrag angenommen: %s`.
static PLACEHOLDER_END: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"{PY_SPACE}*:?{PY_SPACE}*%[sd]{PY_SPACE}*$")).expect("fester Ausdruck"));

/// Mission and objective at the end of the same log line.
static SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"MissionId:{PY_SPACE}*\[([^\]]*)\][^\n]*?ObjectiveId:{PY_SPACE}*\[([^\]]*)\]"))
        .expect("fester Ausdruck")
});

/// The silent end of a mission: `<EndMission> … MissionId[…]`.
static END_MISSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<EndMission>[^\n]*?MissionId\[([^\]]*)\]").expect("fester Ausdruck"));

/// Leaving the game world: back to the main menu or quitting.
static LEFT_GAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"CSessionManager::RequestFrontEnd\]{PY_SPACE}*Started")).expect("fester Ausdruck")
});

static OBJECTIVE_STATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"<ObjectiveUpserted>[^\n]*?mission_id ({PY_NONSPACE}+) - objective_id ({PY_NONSPACE}+) - state MISSION_OBJECTIVE_STATE_(\w+)[^\n]*?flags=({PY_NONSPACE}*)"
    ))
    .expect("fester Ausdruck")
});

static OBJECTIVE_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r#"Added notification "[^"\n]*?:{PY_SPACE}*(.+?){PY_SPACE}*:{PY_SPACE}*"[^\n]*?ObjectiveId: \[([0-9a-fA-F][0-9a-fA-F-]{{7,}})\]"#
    ))
    .expect("fester Ausdruck")
});

/// The literal parts the silent end and leaving the world start with; a text
/// without them cannot match.
const MARK_END_MISSION: &str = "<EndMission>";
const MARK_LEFT_GAME: &str = "CSessionManager::RequestFrontEnd]";
const MARK_OBJECTIVE: &str = "<ObjectiveUpserted>";
const MARK_MARKER: &str = "CreateMarker";

/// The literal parts of the lines the functions `*_from_text` look at: a line without one of them changes no
/// answer, so a reader may keep these lines alone. The change of server is not among them, see [`SilenceScan`].
pub const TEXT_MARKERS: [&str; 5] = [NOTIFICATION, MARK_LEFT_GAME, MARK_END_MISSION, MARK_OBJECTIVE, MARK_MARKER];

/// `Auftrag angenommen: %s` → `Auftrag angenommen`.
fn shorten_phrase(value: &str) -> String {
    py_strip(&PLACEHOLDER_END.replace_all(&clean(value), "")).to_string()
}

/// The wording for one or more `global.ini` keys: first from the language
/// files (each once, in the order found), then the fallback table.
pub fn phrases_for(ini_files: &[PathBuf], keys: &[&str], fallback: &[&[&str]]) -> Vec<String> {
    let prefixes: Vec<String> = keys.iter().map(|k| format!("{k}=")).collect();
    let mut found: Vec<String> = Vec::new();
    for path in ini_files {
        let Ok(bytes) = std::fs::read(path) else { continue };
        // ⚠ Not `utf-8-sig`: a byte order mark stays in front of the first key.
        let text = decode_ignore(&bytes);
        for line in lines_universal(&text) {
            if prefixes.iter().any(|p| line.starts_with(p.as_str())) {
                let value = shorten_phrase(line.split_once('=').map_or("", |(_, v)| v));
                if !value.is_empty() && !found.contains(&value) {
                    found.push(value);
                }
            }
        }
    }
    for candidates in fallback {
        for p in *candidates {
            if !found.iter().any(|f| f == p) {
                found.push((*p).to_string());
            }
        }
    }
    found
}

/// How a contract starts: accepted or shared.
pub fn start_phrases(ini_files: &[PathBuf]) -> Vec<String> {
    phrases_for(ini_files, &INI_KEYS, &START_TABLE)
}

/// How the three ends are called.
pub fn end_phrases(ini_files: &[PathBuf]) -> Vec<String> {
    phrases_for(ini_files, &INI_END_KEYS, &END_TABLE)
}

fn frame(phrases: &[String]) -> Regex {
    let alternatives: Vec<String> = phrases.iter().map(|p| regex::escape(p)).collect();
    Regex::new(&format!(
        r#"Added notification "(?:{}):{PY_SPACE}*(.+?){PY_SPACE}*:{PY_SPACE}*""#,
        alternatives.join("|")
    ))
    .expect("aus maskierten Teilen gebaut")
}

/// The search expressions for the log lines. Two on purpose: the lists come
/// from different keys, and a mix-up would let a completed contract count as new.
#[derive(Clone, Debug)]
pub struct Patterns {
    pub start: Regex,
    pub end: Regex,
}

impl Patterns {
    pub fn build(ini_files: &[PathBuf]) -> Patterns {
        Patterns::of(&start_phrases(ini_files), &end_phrases(ini_files))
    }

    pub fn of(start: &[String], end: &[String]) -> Patterns {
        Patterns { start: frame(start), end: frame(end) }
    }
}

/// What a message says happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A contract was accepted.
    Accept,
    /// A contract (or an objective, see the objective id) ended.
    End,
    /// The game world was left: everything open is gone.
    Left,
}

/// One message: `(is_accept, title, mission_id, objective_id)` of Python.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub kind: Kind,
    pub title: String,
    pub mission_id: String,
    pub objective_id: String,
}

/// Mission and objective id of the message that starts at `position`; both
/// stand at the end of the same line. Missing: two empty texts.
pub fn identifiers(text: &str, position: usize) -> (String, String) {
    let end = text[position..].find('\n').map_or(text.len(), |i| position + i);
    match SUFFIX.captures(&text[position..end]) {
        Some(c) => (py_strip(&c[1]).to_string(), py_strip(&c[2]).to_string()),
        None => (String::new(), String::new()),
    }
}

/// All contract messages of this text, in the order of the log.
///
/// ⚠ The place a message was found is the truth about what happened first.
/// ⚠ An acceptance with an unresolved `~mission(...)` is dropped here, at the
/// source: the game sends the same contract twice, once shared with the
/// placeholder and once accepted with the real name. An end with a
/// placeholder must pass, otherwise the contract stays open for ever.
/// ⚠ An acceptance with the null id is a shared offer, no acceptance.
pub fn events_from_text(text: &str, patterns: &Patterns) -> Vec<Event> {
    let mut found: Vec<(usize, Kind, String, Option<String>)> = Vec::new();
    if text.contains(NOTIFICATION) {
        for c in patterns.start.captures_iter(text) {
            let at = c.get(0).map_or(0, |m| m.start());
            found.push((at, Kind::Accept, c[1].to_string(), None));
        }
        for c in patterns.end.captures_iter(text) {
            let at = c.get(0).map_or(0, |m| m.start());
            found.push((at, Kind::End, c[1].to_string(), None));
        }
    }
    if text.contains(MARK_LEFT_GAME) {
        found.extend(LEFT_GAME.find_iter(text).map(|m| (m.start(), Kind::Left, String::new(), None)));
    }
    // The silent end: no title, but the id.
    if text.contains(MARK_END_MISSION) {
        for c in END_MISSION.captures_iter(text) {
            let at = c.get(0).map_or(0, |m| m.start());
            found.push((at, Kind::End, String::new(), Some(py_strip(&c[1]).to_string())));
        }
    }
    found.sort_by_key(|f| f.0);
    let mut result = Vec::new();
    for (position, kind, title, mission) in found {
        if kind == Kind::Accept && PLACEHOLDER.is_match(&title) {
            continue;
        }
        match mission {
            None => {
                let (mission_id, objective_id) = identifiers(text, position);
                if kind == Kind::Accept && mission_id == NULL_ID {
                    continue;
                }
                result.push(Event { kind, title, mission_id, objective_id });
            }
            // An `EndMission` ends the whole contract, never an objective.
            Some(mission_id) => result.push(Event { kind, title, mission_id, objective_id: String::new() }),
        }
    }
    result
}

/// Which open contract does this end close, or none?
///
/// Three steps in this order: an objective id means only an objective ended
/// and the contract goes on; else over the title; else over the mission id.
pub fn which_ended<'a>(
    plain: &str,
    mission_id: &str,
    objective_id: &str,
    pending: impl IntoIterator<Item = &'a str>,
    mission_keys: &std::collections::HashMap<String, String>,
) -> Option<String> {
    if !objective_id.is_empty() {
        return None;
    }
    if pending.into_iter().any(|name| name == plain) {
        return Some(plain.to_string());
    }
    if mission_id.is_empty() { None } else { mission_keys.get(mission_id).cloned() }
}

/// The raw message about an objective.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectiveEvent {
    /// `(mission_id, objective_id, state, flags)` from the language-neutral line.
    State { mission_id: String, objective_id: String, state: String, flags: String },
    /// The wording of an objective, matched by its id.
    Title { objective_id: String, wording: String },
}

/// The states of the objectives in this text, in the order of the log.
pub fn objective_states_from_text(text: &str) -> Vec<(String, String, String, String)> {
    if !text.contains(MARK_OBJECTIVE) {
        return Vec::new();
    }
    OBJECTIVE_STATE
        .captures_iter(text)
        .map(|c| (c[1].to_string(), c[2].to_string(), c[3].to_string(), c[4].to_string()))
        .collect()
}

/// All objective messages of this text, in the order of the log.
pub fn objective_events_from_text(text: &str) -> Vec<ObjectiveEvent> {
    let mut found: Vec<(usize, ObjectiveEvent)> = Vec::new();
    if text.contains(MARK_OBJECTIVE) {
        for c in OBJECTIVE_STATE.captures_iter(text) {
            let at = c.get(0).map_or(0, |m| m.start());
            found.push((
                at,
                ObjectiveEvent::State {
                    mission_id: c[1].to_string(),
                    objective_id: c[2].to_string(),
                    state: c[3].to_string(),
                    flags: c[4].to_string(),
                },
            ));
        }
    }
    if text.contains(NOTIFICATION) && text.contains("ObjectiveId: [") {
        for c in OBJECTIVE_TITLE.captures_iter(text) {
            let at = c.get(0).map_or(0, |m| m.start());
            found.push((
                at,
                ObjectiveEvent::Title { objective_id: c[2].to_string(), wording: py_strip(&c[1]).to_string() },
            ));
        }
    }
    found.sort_by_key(|f| f.0);
    found.into_iter().map(|(_, e)| e).collect()
}

// ---------------------------------------------------------------------------
// Which contract is behind a mission, a change of server
// ---------------------------------------------------------------------------

/// `CreateMarker … missionId [x] … contractDefinitionId[y]`: the contract of the catalog behind a mission.
///
/// ⚠ `missionId` stands in lower case and without a colon here, unlike in the
/// suffix of the notifications: two spellings of the same thing in one log.
static CONTRACT_MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"CreateMarker[^\n]*?missionId \[([0-9a-fA-F-]+)\][^\n]*?contractDefinitionId\[([0-9a-fA-F-]+)\]")
        .expect("fester Ausdruck")
});

/// `{mission_id: contract_id}`: which contract is behind the mission?
///
/// ⚠ The **first** marker wins: a contract sets several objectives, all with
/// the same contract id.
pub fn contracts_from_text(text: &str) -> HashMap<String, String> {
    let mut found = HashMap::new();
    if !text.contains(MARK_MARKER) {
        return found;
    }
    for c in CONTRACT_MARKER.captures_iter(text) {
        found.entry(c[1].to_string()).or_insert_with(|| c[2].to_string());
    }
    found
}

/// A change of server. After it the game reports lines with their mission id
/// again for the contracts that go on; most stay silent and are gone without
/// an end in the log.
const SERVER_CHANGE: &str = "<Change Server Start>";

/// `(changed, silent, alive)`: has a change of server happened, which mission ids are silent since, which alive.
pub type Silence = (bool, HashSet<String>, HashSet<String>);

/// `(changed, silent, alive)` for a log text: has a change of server happened,
/// which of the ids no longer appear after the last one, which do (or, without
/// a change, appear anywhere in the text).
pub fn server_change_silence(text: &str, mission_ids: &[String]) -> Silence {
    let mut scan = SilenceScan::new(mission_ids);
    scan.piece(text);
    scan.finish()
}

/// [`server_change_silence`] for a log that is handed over a piece at a time (whole lines): the answer is the same
/// as for the pieces joined into one text.
pub struct SilenceScan {
    ids: HashSet<String>,
    changed: bool,
    /// The ids found after the last change of server so far.
    after: HashSet<String>,
    /// The ids found anywhere so far, counted until the first change of server.
    anywhere: HashSet<String>,
}

impl SilenceScan {
    pub fn new(mission_ids: &[String]) -> SilenceScan {
        SilenceScan {
            ids: mission_ids.iter().filter(|i| !i.is_empty()).cloned().collect(),
            changed: false,
            after: HashSet::new(),
            anywhere: HashSet::new(),
        }
    }

    /// The next piece of the log.
    pub fn piece(&mut self, text: &str) {
        let from = match text.rfind(SERVER_CHANGE) {
            Some(last) => {
                self.changed = true;
                self.after.clear();
                last
            }
            None => 0,
        };
        for id in &self.ids {
            if self.changed && !self.after.contains(id) && text[from..].contains(id.as_str()) {
                self.after.insert(id.clone());
            }
            if !self.changed && !self.anywhere.contains(id) && text.contains(id.as_str()) {
                self.anywhere.insert(id.clone());
            }
        }
    }

    /// `(changed, silent, alive)`.
    pub fn finish(self) -> Silence {
        if !self.changed {
            return (false, HashSet::new(), self.anywhere);
        }
        let silent = self.ids.difference(&self.after).cloned().collect();
        (true, silent, self.after)
    }
}

/// Mission id → contract key in the order the ids were first set, like a
/// Python dictionary: setting an id again keeps its place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MissionKeys {
    order: Vec<String>,
    map: HashMap<String, String>,
}

impl MissionKeys {
    pub fn insert(&mut self, id: &str, key: &str) {
        if self.map.insert(id.to_string(), key.to_string()).is_none() {
            self.order.push(id.to_string());
        }
    }

    pub fn get(&self, id: &str) -> Option<&String> {
        self.map.get(id)
    }

    pub fn remove(&mut self, id: &str) {
        if self.map.remove(id).is_some() {
            self.order.retain(|i| i != id);
        }
    }

    /// The ids that belong to a contract, in order.
    pub fn ids_of(&self, key: &str) -> Vec<String> {
        self.order.iter().filter(|i| self.map.get(*i).is_some_and(|k| k == key)).cloned().collect()
    }

    /// Takes away every id of a contract; the ids are returned.
    pub fn remove_key(&mut self, key: &str) -> Vec<String> {
        let ids = self.ids_of(key);
        for id in &ids {
            self.remove(id);
        }
        ids
    }

    pub fn clear(&mut self) {
        self.order.clear();
        self.map.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `(id, key)` in the order of first setting.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.order.iter().filter_map(|i| self.map.get(i).map(|k| (i, k)))
    }

    pub fn as_map(&self) -> &HashMap<String, String> {
        &self.map
    }
}

/// What is open according to this text: the titles as they stand in the
/// log, and which mission ids belong to which contract (`state_from_text`).
///
/// ⚠ A contract that ends without a title is found over its mission id; an
/// acceptance without a title is worthless. The null id is no id: a shared
/// offer would throw all shared contracts into one pot.
pub fn state_from_text(text: &str, patterns: &Patterns) -> (Vec<String>, MissionKeys) {
    let mut pending: Vec<(String, String)> = Vec::new();
    let mut keys = MissionKeys::default();
    for event in events_from_text(text, patterns) {
        if event.kind == Kind::Left {
            pending.clear();
            keys.clear();
            continue;
        }
        let plain = clean(&event.title);
        let accept = event.kind == Kind::Accept;
        if plain.is_empty() && !(!event.mission_id.is_empty() && !accept) {
            continue;
        }
        if accept {
            if !pending.iter().any(|(p, _)| *p == plain) {
                pending.push((plain.clone(), event.title.clone()));
            }
            if !event.mission_id.is_empty() && event.mission_id != NULL_ID {
                keys.insert(&event.mission_id, &plain);
            }
            continue;
        }
        let gone = which_ended(
            &plain,
            &event.mission_id,
            &event.objective_id,
            pending.iter().map(|(p, _)| p.as_str()),
            keys.as_map(),
        );
        let Some(gone) = gone else { continue };
        pending.retain(|(p, _)| *p != gone);
        keys.remove_key(&gone);
    }
    (pending.into_iter().map(|(_, title)| title).collect(), keys)
}

/// At most this many objectives stand under a contract.
pub const OBJECTIVES_MAX: usize = 6;

/// The flag of an objective the game itself writes into the contract book.
const OBJECTIVE_VISIBLE: &str = "ShowInLog";

/// What there is to do for the contracts: a state, no history. `absorb` eats
/// a section after section, `open_for` says what is up right now.
#[derive(Clone, Debug, Default)]
pub struct Objectives {
    titles: HashMap<String, String>,
    /// Mission id → objectives in the order of the log: `(objective id, state, flags)`.
    states: HashMap<String, Vec<(String, String, String)>>,
}

impl Objectives {
    /// Books a section. True if something changed.
    ///
    /// ⚠ The answer matters: objectives change without the list of contracts
    /// changing. Without it the bar would show the objective from twenty minutes ago.
    pub fn absorb(&mut self, events: &[ObjectiveEvent]) -> bool {
        let mut changed = false;
        for event in events {
            match event {
                ObjectiveEvent::Title { objective_id, wording } => {
                    if !wording.is_empty() && self.titles.get(objective_id) != Some(wording) {
                        self.titles.insert(objective_id.clone(), wording.clone());
                        changed = true;
                    }
                }
                ObjectiveEvent::State { mission_id, objective_id, state, flags } => {
                    let per_mission = self.states.entry(mission_id.clone()).or_default();
                    match per_mission.iter_mut().find(|(oid, ..)| oid == objective_id) {
                        Some(entry) => {
                            if entry.1 != *state || entry.2 != *flags {
                                entry.1.clone_from(state);
                                entry.2.clone_from(flags);
                                changed = true;
                            }
                        }
                        None => {
                            per_mission.push((objective_id.clone(), state.clone(), flags.clone()));
                            changed = true;
                        }
                    }
                }
            }
        }
        changed
    }

    /// The open objectives of a contract in the order of the log.
    ///
    /// ⚠ Without a wording nothing is shown: an objective whose message was
    /// never seen gets no line.
    pub fn open_for(&self, mission_id: Option<&str>) -> Vec<String> {
        let Some(entries) = mission_id.and_then(|m| self.states.get(m)) else { return Vec::new() };
        let mut names: Vec<String> = Vec::new();
        for (oid, state, flags) in entries {
            if state != "INPROGRESS" || !flags.contains(OBJECTIVE_VISIBLE) {
                continue;
            }
            if let Some(wording) = self.titles.get(oid).filter(|w| !w.is_empty())
                && !names.contains(wording)
            {
                names.push(wording.clone());
            }
        }
        names
    }

    /// The contract is over, its objectives too.
    pub fn forget(&mut self, mission_id: &str) {
        self.states.remove(mission_id);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn patterns() -> Patterns {
        Patterns::of(&start_phrases(&[]), &end_phrases(&[]))
    }

    fn line(phrase: &str, title: &str, ids: &str) -> String {
        format!(
            "<2026-10-01T10:00:00.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{phrase}: {title}: \" [1] {ids}\n"
        )
    }

    fn ids(mission: &str, objective: &str) -> String {
        format!("MissionId: [{mission}], ObjectiveId: [{objective}]")
    }

    #[test]
    fn the_table_is_the_fallback_without_a_language_file() {
        assert_eq!(
            start_phrases(&[]),
            [
                "Auftrag angenommen",
                "Auftrag geteilt",
                "Uftrag angenommen",
                "Uftrag geteilt",
                "Contract Accepted",
                "Contract Shared"
            ]
        );
        assert_eq!(end_phrases(&[]).len(), 9);
        assert!(!end_phrases(&[]).iter().any(|p| p.contains("geteilt")));
    }

    #[test]
    fn the_language_file_comes_first_and_the_placeholder_goes() {
        let dir = std::env::temp_dir().join(format!("versekit-contracts-{}", crate::testdir::run_tag()));
        std::fs::create_dir_all(&dir).expect("Ordner");
        let ini = dir.join("global.ini");
        std::fs::write(
            &ini,
            "\u{feff}mobiGlas_ui_MissionEvent_Activated=Neuer Auftrag: %s\r\n\
             mobiGlas_ui_Mission_Shared=Auftrag <EM4>[BP]</EM4> geteilt  :  %d \n\
             mobiGlas_ui_MissionEvent_Complete=Fertig: %s\n\
             mobiGlas_ui_MissionEvent_Complete=Fertig: %s\n",
        )
        .expect("ini");
        let start = start_phrases(std::slice::from_ref(&ini));
        // The byte order mark stays in front of the first key, which is then unknown.
        assert_eq!(&start[..2], ["Auftrag geteilt", "Auftrag angenommen"]);
        assert_eq!(end_phrases(std::slice::from_ref(&ini))[0], "Fertig");
        assert_eq!(end_phrases(std::slice::from_ref(&ini)).iter().filter(|p| *p == "Fertig").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn accept_end_and_shared_in_the_order_of_the_log() {
        let text = format!(
            "{}{}{}",
            line("Auftrag angenommen", "Retake Platforms", &ids("aaaa", "")),
            line("Auftrag zurückgezogen", "Obere Plattform erreichen", &ids("aaaa", "o1")),
            line("Auftrag abgeschlossen", "Retake Platforms", &ids("aaaa", "")),
        );
        let events = events_from_text(&text, &patterns());
        let kinds: Vec<(Kind, &str, &str, &str)> =
            events.iter().map(|e| (e.kind, e.title.as_str(), e.mission_id.as_str(), e.objective_id.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (Kind::Accept, "Retake Platforms", "aaaa", ""),
                (Kind::End, "Obere Plattform erreichen", "aaaa", "o1"),
                (Kind::End, "Retake Platforms", "aaaa", ""),
            ]
        );
    }

    #[test]
    fn a_shared_offer_and_a_placeholder_acceptance_are_no_acceptance() {
        let shared = line("Auftrag geteilt", "Stop Rival Attack at Base", &ids(NULL_ID, ""));
        assert!(events_from_text(&shared, &patterns()).is_empty());
        let raw = line("Auftrag angenommen", "Stop Rival Attack at ~mission(Location)", &ids("bbbb", ""));
        assert!(events_from_text(&raw, &patterns()).is_empty());
        // An end with a placeholder must pass.
        let end = line("Auftrag abgeschlossen", "Stop Rival Attack at ~mission(Location)", &ids("bbbb", ""));
        assert_eq!(events_from_text(&end, &patterns()).len(), 1);
        // A shared offer with a real id still counts as a start.
        let real = line("Auftrag geteilt", "Hunt", &ids("cccc", ""));
        assert_eq!(events_from_text(&real, &patterns())[0].kind, Kind::Accept);
    }

    #[test]
    fn the_silence_of_pieces_is_the_silence_of_the_whole_text() {
        let ids: Vec<String> = ["id-a", "id-b", "id-c", ""].map(String::from).to_vec();
        let texts = [
            "x id-a\n<Change Server Start>\nid-b y\n",
            "id-a\nid-b\nid-c\n",
            "<Change Server Start> id-a\nid-b\n<Change Server Start>\nid-c id-b\n id-a <Change Server Start>",
            "id-a id-b <Change Server Start> id-c\n",
            "no id at all\n",
            "",
        ];
        for text in texts {
            let want = server_change_silence(text, &ids);
            for (cut, _) in text.match_indices('\n') {
                let mut scan = SilenceScan::new(&ids);
                scan.piece(&text[..=cut]);
                scan.piece(&text[cut + 1..]);
                assert_eq!(scan.finish(), want, "{text:?} cut after {cut}");
            }
            let mut scan = SilenceScan::new(&ids);
            text.split_inclusive('\n').for_each(|line| scan.piece(line));
            assert_eq!(scan.finish(), want, "{text:?} line by line");
        }
        // Something to see in the answer: after the last change only the ids named after it live.
        let (changed, silent, alive) = server_change_silence(texts[0], &ids);
        assert!(changed);
        assert_eq!(alive, HashSet::from(["id-b".to_string()]));
        assert_eq!(silent, HashSet::from(["id-a".to_string(), "id-c".to_string()]));
        // Without a change every id named anywhere lives.
        let (changed, silent, alive) = server_change_silence(texts[1], &ids);
        assert!(!changed && silent.is_empty());
        assert_eq!(alive.len(), 3);
    }

    #[test]
    fn the_silent_end_and_leaving_the_world() {
        let text = "<2026-10-01T10:00:00.000Z> <EndMission> Foo MissionId[ e0b9 ] x CompletionType[Abandon]\n\
                    <2026-10-01T10:00:01.000Z> [CSessionManager::RequestFrontEnd]  Started - Reason\n\
                    <2026-10-01T10:00:02.000Z> [CSessionManager::RequestFrontEnd] Beendet\n";
        let events = events_from_text(text, &patterns());
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            Event { kind: Kind::End, title: String::new(), mission_id: "e0b9".into(), objective_id: String::new() }
        );
        assert_eq!(events[1].kind, Kind::Left);
    }

    #[test]
    fn which_ended_goes_by_title_then_by_id_and_never_by_objective() {
        let mut keys = HashMap::new();
        keys.insert("m1".to_string(), "Alter Titel".to_string());
        let open = ["Alter Titel", "Neu"];
        assert_eq!(which_ended("Neu", "", "o", open, &keys), None);
        assert_eq!(which_ended("Neu", "x", "", open, &keys).as_deref(), Some("Neu"));
        assert_eq!(which_ended("Anders", "m1", "", open, &keys).as_deref(), Some("Alter Titel"));
        assert_eq!(which_ended("Anders", "m2", "", open, &keys), None);
        assert_eq!(which_ended("Anders", "", "", open, &keys), None);
    }

    #[test]
    fn objective_states_and_wording() {
        let text = "<2026-10-01T10:00:00.000Z> <ObjectiveUpserted> x mission_id m1 - objective_id o1 - state MISSION_OBJECTIVE_STATE_INPROGRESS - flags=ShowInLog|Hidden y\n\
                    <2026-10-01T10:00:01.000Z> Added notification \"Neues Ziel: Fliege los: \" [1] MissionId: [m1], ObjectiveId: [0123abcd-1]\n";
        let states = objective_states_from_text(text);
        assert_eq!(states, [("m1".into(), "o1".into(), "INPROGRESS".into(), "ShowInLog|Hidden".into())]);
        let all = objective_events_from_text(text);
        assert_eq!(all.len(), 2);
        assert_eq!(all[1], ObjectiveEvent::Title { objective_id: "0123abcd-1".into(), wording: "Fliege los".into() });
        assert!(objective_states_from_text("nichts").is_empty());
    }
}
