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

//! The mission log's reading of the game logs: which contracts were accepted
//! when and how they ended, from all backups and the running `Game.log`
//! (`mission_log.scan_backlog`, `from_files`, `_read`, `_close_expired`,
//! `_assign_bp`, `reassess`). It writes `auftragslog.json` with the read
//! position and starts the catching up of the playtime and the statistics.
//!
//! ⚠ This module recognises no contract itself: every recognition comes from
//! `contracts`, so two readings of the same lines cannot drift apart at the
//! next patch. What it adds is the time of each line, the course over
//! several sessions, completed against abandoned, and the count.
//!
//! ⚠ The game sends the same acceptance twice in the same millisecond with
//! different numbers in the brackets; it is told apart by (time, title, kind).

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;

use crate::contracts::{self, Event, Kind, Patterns};
use crate::errors;
use crate::jsonio::{self, Map, Value, map_set};
use crate::location;
use crate::logsource::{Clock, LogSource};
use crate::logtime::{Facts, candidate_lines, seconds, time_of};
use crate::mission_log::{self as log, ABORTED, COMPLETED, EXPIRED, FAILED, RUNNING, clean, get, key_of, text_of};
use crate::ordered::{OrderedMap, parallel_map};
use crate::paths::{Paths, now_seconds};
use crate::play_stats;
use crate::playtime;
use crate::pypath::Flavor;
use crate::stock_journal;
use crate::text::{decode_replace, lines_universal};

/// How long a session must have run for its silence to prove something.
///
/// ⚠ A session without any contract says little: a failed start names none
/// either. A long, complete one without any contract proves that none is
/// open. Spawn alone is not enough: short failed starts do not always name
/// the contract. One honest dead entry is better than a contract closed wrongly.
pub const SESSION_COUNTS_SEC: i64 = 90 * 60;
/// How long after handing in a blueprint still belongs to the contract: the
/// reward falls after the end, not before it.
pub const BP_GRACE_SEC: i64 = 300;

/// Something went wrong that Python would have raised on. The contract log is
/// not written then; the playtime and the statistics may already be.
#[derive(Debug, PartialEq, Eq)]
pub struct ScanError(pub String);

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ScanError {}

/// The end of a mission and how it ended: `CompletionType[Complete|Abandon|…]`.
static END_KIND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<EndMission>.*?MissionId\[([^\]]*)\].*?CompletionType\[([^\]]*)\]").expect("fester Ausdruck")
});

/// The lines the reading looks at; every other line cannot change anything.
static MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    let literals =
        ["<EndMission>", "<ObjectiveUpserted>", "Added notification \"", "CSessionManager::RequestFrontEnd]"];
    let escaped: Vec<String> = literals.iter().map(|l| regex::escape(l)).collect();
    Regex::new(&escaped.join("|")).expect("aus maskierten Teilen gebaut")
});

/// The lines the titles of the silent-session question come from.
static NOTIFICATIONS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&regex::escape("Added notification \"")).expect("aus maskiertem Teil gebaut"));

/// Phase tags of the already-seen set.
const SEEN_ACCEPT: u8 = 0;
const SEEN_END: u8 = 1;
const SEEN_LEFT: u8 = 2;
const SEEN_BLUEPRINT: u8 = 3;

/// The state for `CompletionType[…]`.
///
/// ⚠ Failing is not completing: four outcomes exist (Complete, Abandon, Fail,
/// Deactivate). `Deactivate` means the game withdrew the
/// contract itself, nothing to the player's credit or blame, so `verfallen`.
/// An unknown or missing outcome stays completed, as before: it covers every
/// end that stands only as a notice in the log.
pub fn state_for(kind: &str) -> &'static str {
    let kind = kind.to_lowercase();
    if kind.starts_with("abandon") {
        ABORTED
    } else if kind.starts_with("fail") {
        FAILED
    } else if kind.starts_with("deactivate") {
        EXPIRED
    } else {
        COMPLETED
    }
}

// ---------------------------------------------------------------- one log

/// What one line of a log said, in the order of the lines.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Item {
    /// `<EndMission>`: how the mission ended, noted before the notice comes.
    EndKind { mission: String, kind: String },
    /// The state of an objective.
    Objective { mission: String, objective: String, state: String },
    /// A blueprint found, name cleaned.
    Blueprint { name: String, when: String },
    /// A contract message.
    Event { when: String, event: Event },
}

/// Everything a log says about contracts, read without any bookkeeping.
struct FileScan {
    items: Vec<Item>,
    facts: Facts,
    /// The first stamp of the head: when the session was played.
    session: String,
}

fn scan_text(text: &str, patterns: &Patterns, blueprints: &Regex) -> (Vec<Item>, Facts) {
    let mut items = Vec::new();
    for line in candidate_lines(text, &MARKERS) {
        if line.contains("<EndMission>")
            && let Some(c) = END_KIND.captures(line)
        {
            items.push(Item::EndKind { mission: c[1].to_string(), kind: c[2].to_string() });
        }
        for (mission, objective, state, _flags) in contracts::objective_states_from_text(line) {
            items.push(Item::Objective { mission, objective, state });
        }
        // ⭐ Which blueprint came from which contract: found with the same
        // pattern as in the stock; the time of the line does the assigning.
        if line.contains("Added notification \"") {
            for hit in blueprints.captures_iter(line) {
                let raw = hit.iter().skip(1).flatten().map(|m| m.as_str()).find(|s| !s.is_empty()).unwrap_or("");
                let name = clean(raw);
                if !name.is_empty() {
                    items.push(Item::Blueprint { name, when: time_of(line).to_string() });
                }
            }
        }
        for event in contracts::events_from_text(line, patterns) {
            items.push(Item::Event { when: time_of(line).to_string(), event });
        }
    }
    (items, Facts::of(text))
}

/// When this session was played: the first stamp near the top of the log.
///
/// ⚠ Not the modification time of the file: on a backup that is the time of
/// copying, and logs saved in one go all carry the same. Not the name either:
/// `30 Aug 26` sorts wrongly. Without a stamp at the end: better at the back
/// than twisting the row.
pub fn session_start_of(path: &str) -> String {
    const END: &str = "9999";
    let mut limit: usize = 64 * 1024;
    loop {
        let Ok(file) = File::open(path) else { return END.to_string() };
        let mut bytes = Vec::new();
        if file.take(limit as u64).read_to_end(&mut bytes).is_err() {
            return END.to_string();
        }
        let complete = bytes.len() < limit;
        let text = decode_replace(&bytes);
        let lines = lines_universal(&text);
        // The 200 first lines must be whole ones: the last of a cut text may not be.
        if complete || lines.len() > 200 {
            return lines.iter().take(200).map(|l| time_of(l)).find(|t| !t.is_empty()).unwrap_or(END).to_string();
        }
        limit *= 4;
    }
}

// ----------------------------------------------------- the bookkeeping

/// One run of a contract in the making.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Run {
    name: String,
    when: String,
    state: &'static str,
    goals_done: i64,
    goals_total: i64,
    blueprints: Vec<String>,
    source: String,
    until: Option<String>,
}

impl Run {
    fn new(name: &str, when: &str, source: &str) -> Run {
        Run {
            name: name.to_string(),
            when: when.to_string(),
            state: RUNNING,
            goals_done: 0,
            goals_total: 0,
            blueprints: Vec::new(),
            source: source.to_string(),
            until: None,
        }
    }

    /// The entry as the file holds it; `bis` only once it ended.
    fn to_map(&self) -> Map {
        let mut map: Map = vec![
            ("name".into(), Value::str(&self.name)),
            ("wann".into(), Value::str(&self.when)),
            ("zustand".into(), Value::str(self.state)),
            ("ziele_fertig".into(), Value::int(self.goals_done)),
            ("ziele_gesamt".into(), Value::int(self.goals_total)),
            ("bauplaene".into(), Value::Array(self.blueprints.iter().map(Value::str).collect())),
            ("quelle".into(), Value::str(&self.source)),
        ];
        if let Some(until) = &self.until {
            map.push(("bis".into(), Value::str(until)));
        }
        map
    }
}

/// A list of entries the open ones are counted by position in.
trait Arena {
    fn name(&self, i: usize) -> &str;
    fn when(&self, i: usize) -> &str;
    fn expire(&mut self, i: usize);
}

impl Arena for Vec<Run> {
    fn name(&self, i: usize) -> &str {
        &self[i].name
    }
    fn when(&self, i: usize) -> &str {
        &self[i].when
    }
    fn expire(&mut self, i: usize) {
        self[i].state = EXPIRED;
    }
}

/// The saved entries of the log file.
struct Stored<'a>(&'a mut Vec<Map>);

impl Arena for Stored<'_> {
    fn name(&self, i: usize) -> &str {
        text_of(&self.0[i], "name")
    }
    fn when(&self, i: usize) -> &str {
        text_of(&self.0[i], "wann")
    }
    fn expire(&mut self, i: usize) {
        map_set(&mut self.0[i], "zustand", Value::str(EXPIRED));
    }
}

/// Ends the contracts a later session does not know any more.
///
/// ⚠⚠ This is the upper limit the log lacked. Logging out ends no contract,
/// but one day it is over, and without this rule it would stand as running for
/// ever, collecting every blueprint found later. The rule comes from the
/// game: when logging in, Star Citizen reports
/// every contract still running as accepted again. So one that a later session
/// does not name cannot have been open there.
///
/// ⚠ A silent session proves nothing, except a long, complete one
/// (`SESSION_COUNTS_SEC`). ⚠ The state is `verfallen`, not `abgebrochen`:
/// whether it was handed in or given up stands in no log.
fn close_expired(
    arena: &mut dyn Arena,
    pending: &mut Vec<usize>,
    done: &mut Vec<usize>,
    reported: &HashSet<String>,
    session: &str,
    silent_counts: bool,
) {
    if pending.is_empty() {
        return;
    }
    if reported.is_empty() {
        if !silent_counts {
            return;
        }
        for i in std::mem::take(pending) {
            arena.expire(i);
            done.push(i);
        }
        return;
    }
    for i in pending.clone() {
        if reported.contains(arena.name(i)) {
            continue;
        }
        // ⚠ Only what began before this session: one accepted in this very
        // session stands in `reported` anyway, and without this limit the order
        // within a file would count instead of the change of session.
        if arena.when(i) >= session {
            continue;
        }
        arena.expire(i);
        pending.retain(|&p| p != i);
        done.push(i);
    }
}

/// Which contract a found blueprint belongs to.
///
/// ⚠ Running first, only then the one just ended: whoever hands in a contract
/// and takes the next at once gets the reward of the old one while the new one
/// already runs. ⚠⚠ A contract gives at most ONE blueprint, a rule of the game.
/// Who has one is out. If nobody is left, the blueprint belongs to no contract:
/// it may come from crafting or from a contract whose acceptance is in no log
/// any more. ⚠⚠ `reported` are the contracts of THIS session: one still open
/// from an earlier session that the game does not name here does not run any
/// more and gets nothing.
fn assign_blueprint(
    runs: &mut [Run],
    pending: &[usize],
    done: &[usize],
    name: &str,
    when: &str,
    reported: &HashSet<String>,
) -> bool {
    for &i in pending.iter().rev() {
        if !runs[i].blueprints.is_empty() || !reported.contains(&runs[i].name) {
            continue;
        }
        runs[i].blueprints.push(name.to_string());
        return true;
    }
    let Some(now) = seconds(when) else { return false };
    for &i in done.iter().rev() {
        if !runs[i].blueprints.is_empty() {
            continue;
        }
        let end = seconds(runs[i].until.as_deref().unwrap_or(""));
        if let Some(end) = end
            && (0..=BP_GRACE_SEC).contains(&(now - end))
        {
            runs[i].blueprints.push(name.to_string());
            return true;
        }
    }
    false
}

/// The history over several logs: open and ended runs, what was seen already,
/// and the mission ids that tie an ending to its acceptance.
#[derive(Default)]
struct Book {
    runs: Vec<Run>,
    pending: Vec<usize>,
    done: Vec<usize>,
    seen: HashSet<(String, String, u8)>,
    ident: HashMap<String, String>,
}

impl Book {
    /// Reads one log into the book; returns the titles this session names as
    /// accepted (resumptions included) and whether its silence proves something.
    fn read(&mut self, scan: &FileScan, source: &str) -> (HashSet<String>, bool) {
        let mut endings: HashMap<&str, &str> = HashMap::new();
        let mut objectives: OrderedMap<OrderedMap<String>> = OrderedMap::new();
        let mut reported: HashSet<String> = HashSet::new();
        for item in &scan.items {
            match item {
                Item::EndKind { mission, kind } => {
                    endings.insert(mission, kind);
                }
                Item::Objective { mission, objective, state } => {
                    objectives.entry_or_default(mission).set(objective, state.clone());
                }
                Item::Blueprint { name, when } => {
                    // The same double report as below.
                    if self.seen.insert((when.clone(), name.clone(), SEEN_BLUEPRINT)) {
                        assign_blueprint(&mut self.runs, &self.pending, &self.done, name, when, &reported);
                    }
                }
                Item::Event { when, event } => self.event(when, event, source, &endings, &mut reported),
            }
        }
        // Progress only where the assignment is clear: the log ties title and
        // mission id nowhere. With exactly one open contract and one id it can
        // only be this one; otherwise it stays guessing, and a wrong number is
        // worse than none.
        if self.pending.len() == 1
            && objectives.len() == 1
            && let Some(progress) = objectives.values().next()
        {
            // Phase objectives describe the section, not a task to tick off.
            let real: Vec<&String> =
                progress.iter().filter(|(k, _)| !k.starts_with("phase_")).map(|(_, v)| v).collect();
            if !real.is_empty() {
                let run = &mut self.runs[self.pending[0]];
                run.goals_total = real.len() as i64;
                run.goals_done = real.iter().filter(|v| v.to_uppercase().ends_with("COMPLETED")).count() as i64;
            }
        }
        (reported, scan.facts.spawn && scan.facts.duration() >= SESSION_COUNTS_SEC)
    }

    fn event(
        &mut self,
        when: &str,
        event: &Event,
        source: &str,
        endings: &HashMap<&str, &str>,
        reported: &mut HashSet<String>,
    ) {
        // ⚠ ALWAYS through `clean`: the title stands with `<EM4>[BP!]</EM4>` or
        // with the inserted block, depending on what was written into the game.
        // Uncleaned, the same contract counts as two.
        let title = clean(&event.title);
        let tag = match event.kind {
            Kind::Accept => SEEN_ACCEPT,
            Kind::End => SEEN_END,
            Kind::Left => SEEN_LEFT,
        };
        if !self.seen.insert((when.to_string(), title.clone(), tag)) {
            return;
        }
        match event.kind {
            // ⚠⚠ Leaving the world: do NOT clear here. `contracts` clears at this
            // point because the overlay shows nothing after logging out. A log has
            // the opposite job: logging out ends no contract, it is often handed in
            // in the next session. Whoever clears here loses exactly the contracts
            // that went over two evenings.
            Kind::Left => {}
            Kind::Accept => {
                // ⚠ A title with a raw placeholder is no entry: the game fills it in
                // only when showing it, the values stand nowhere, and next to it
                // stood the same contract with the rank resolved.
                if title.is_empty() || title.contains("~mission(") {
                    return;
                }
                // ⚠ Before the resumption check: the resumption is the proof that the
                // contract still ran in this session.
                reported.insert(title.clone());
                // ⚠⚠ A resumption is no new acceptance: when logging in, the game
                // reports every running contract as accepted again. Without this
                // check a contract would stand once per session it was open in.
                let id = &event.mission_id;
                let open_with = |name: &str| self.pending.iter().any(|&i| self.runs[i].name == name);
                let again =
                    open_with(&title) || (!id.is_empty() && self.ident.get(id).is_some_and(|known| open_with(known)));
                if again {
                    return;
                }
                self.runs.push(Run::new(&title, when, source));
                self.pending.push(self.runs.len() - 1);
                if !id.is_empty() {
                    self.ident.insert(id.clone(), title);
                }
            }
            Kind::End => {
                // ⚠⚠ An end, but WHICH? `which_ended` decides, not this module. Its first
                // step is the decisive one: an objective id means only an objective
                // ended and the contract goes on. And if nothing can be assigned,
                // NOTHING is entered: an invented contract is worse than a missing one.
                let names = self.pending.iter().map(|&i| self.runs[i].name.as_str());
                let hit = contracts::which_ended(&title, &event.mission_id, &event.objective_id, names, &self.ident);
                let Some(hit) = hit.filter(|h| !h.is_empty()) else { return };
                let state = state_for(endings.get(event.mission_id.as_str()).copied().unwrap_or(""));
                // ⚠ The OLDEST fitting one, not the youngest: else a contract gets the end
                // of a later run and the log shows an end before its start.
                if let Some(at) = self.pending.iter().position(|&i| self.runs[i].name == hit) {
                    let i = self.pending.remove(at);
                    self.runs[i].state = state;
                    self.runs[i].until = Some(when.to_string());
                    self.done.push(i);
                }
            }
        }
    }

    /// The runs, newest first: ended ones before open ones, then by start
    /// (stable, so equal times keep their order).
    fn finish(self) -> Vec<Run> {
        let mut order: Vec<usize> = self.done.iter().chain(&self.pending).copied().collect();
        order.sort_by(|&a, &b| self.runs[b].when.cmp(&self.runs[a].when));
        order.into_iter().map(|i| self.runs[i].clone()).collect()
    }
}

// ----------------------------------------------------------- the scanner

/// What the patterns of a run are built from.
struct Context {
    patterns: Patterns,
    blueprints: Regex,
}

type Stamp = (PathBuf, u64, Option<std::time::SystemTime>);
/// A log's file, state and the two patterns the titles were found with.
type ReportedKey = (String, i128, u64, String, String);

/// Reads the game logs for the mission log, with the memory of one program run.
pub struct MissionScan<'s, 'p> {
    src: &'s LogSource<'p>,
    stats_clock: Clock,
    context: Mutex<Option<(Vec<Stamp>, Arc<Context>)>>,
    starts: Mutex<HashMap<(String, i128, u64), String>>,
    /// The titles a session names, per file and state: an old log never changes.
    reported: Mutex<HashMap<ReportedKey, (HashSet<String>, bool)>>,
}

impl<'s, 'p> MissionScan<'s, 'p> {
    pub fn new(src: &'s LogSource<'p>) -> Self {
        Self::with_clock(src, Box::new(now_seconds))
    }

    /// `stats_clock` is the time the statistics file writes as its `stand`.
    pub fn with_clock(src: &'s LogSource<'p>, stats_clock: Clock) -> Self {
        MissionScan {
            src,
            stats_clock,
            context: Mutex::new(None),
            starts: Mutex::new(HashMap::new()),
            reported: Mutex::new(HashMap::new()),
        }
    }

    fn paths(&self) -> &'p Paths {
        self.src.paths
    }

    /// Phrases and patterns from the installation's language files; kept
    /// while none of them changes.
    fn context(&self) -> Arc<Context> {
        let files = log::ini_files(self.paths());
        // The own phrases (`phrasen.json`) shape the patterns as well.
        let stamp: Vec<Stamp> = files
            .iter()
            .chain(self.src.sources.phrases_file.iter())
            .map(|p| {
                let meta = fs::metadata(p).ok();
                (p.clone(), meta.as_ref().map_or(0, fs::Metadata::len), meta.and_then(|m| m.modified().ok()))
            })
            .collect();
        let mut slot = self.context.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((known, context)) = slot.as_ref()
            && *known == stamp
        {
            return Arc::clone(context);
        }
        let (phrases, _) = self.src.sources.collect();
        let context =
            Arc::new(Context { patterns: Patterns::build(&files), blueprints: crate::phrases::pattern(&phrases) });
        *slot = Some((stamp, Arc::clone(&context)));
        context
    }

    fn session_start(&self, path: &str) -> String {
        let key = fs::metadata(path)
            .ok()
            .and_then(|m| m.modified().ok().map(|t| (path.to_string(), crate::paths::nanos(t), m.len())));
        if let Some(key) = &key
            && let Some(hit) = self.starts.lock().unwrap_or_else(|e| e.into_inner()).get(key)
        {
            return hit.clone();
        }
        let start = session_start_of(path);
        if let Some(key) = key {
            self.starts.lock().unwrap_or_else(|e| e.into_inner()).insert(key, start.clone());
        }
        start
    }

    /// The files by the time of their session, oldest first; equal ones keep their order.
    fn by_session(&self, files: &[String]) -> Vec<String> {
        let starts = parallel_map(files, |p| self.session_start(p));
        let mut order: Vec<usize> = (0..files.len()).collect();
        order.sort_by(|&a, &b| starts[a].cmp(&starts[b]));
        order.into_iter().map(|i| files[i].clone()).collect()
    }

    /// Several logs as ONE history, newest contract first (`from_files`).
    ///
    /// ⚠ An unreadable file is an error and nothing is returned: Python's `_read`
    /// hands back one value instead of two there and the unpacking raises.
    fn history_of(&self, files: &[String], context: &Context) -> Result<Vec<Map>, ScanError> {
        let mut book = Book::default();
        for batch in files.chunks(16) {
            let scans = parallel_map(batch, |path| -> io::Result<FileScan> {
                let bytes = fs::read(path)?;
                let text = decode_replace(&bytes);
                let (items, facts) = scan_text(&text, &context.patterns, &context.blueprints);
                Ok(FileScan { items, facts, session: self.session_start(path) })
            });
            for (path, scan) in batch.iter().zip(scans) {
                let scan = scan.map_err(|e| {
                    errors::record("mission_log.read", &e);
                    ScanError(format!("{path}: {e}"))
                })?;
                let source = Flavor::native().basename(path);
                let (reported, counts) = book.read(&scan, &source);
                let Book { runs, pending, done, .. } = &mut book;
                close_expired(runs, pending, done, &reported, &scan.session, counts);
            }
        }
        Ok(book.finish().iter().map(Run::to_map).collect())
    }

    /// The titles a session names and whether its silence counts, without the
    /// full reading (`_reported_titles`).
    fn reported_titles(&self, path: &str, patterns: &Patterns) -> io::Result<(HashSet<String>, bool)> {
        let key = fs::metadata(path).ok().and_then(|m| {
            m.modified().ok().map(|t| {
                (
                    path.to_string(),
                    crate::paths::nanos(t),
                    m.len(),
                    patterns.start.as_str().to_string(),
                    patterns.end.as_str().to_string(),
                )
            })
        });
        if let Some(key) = &key
            && let Some(hit) = self.reported.lock().unwrap_or_else(|e| e.into_inner()).get(key)
        {
            return Ok(hit.clone());
        }
        let bytes = fs::read(path)?;
        let text = decode_replace(&bytes);
        let mut reported = HashSet::new();
        for line in candidate_lines(&text, &NOTIFICATIONS) {
            for pattern in [&patterns.start, &patterns.end] {
                if let Some(c) = pattern.captures(line) {
                    reported.insert(clean(&c[1]));
                    break;
                }
            }
        }
        let facts = Facts::of(&text);
        let result = (reported, facts.spawn && facts.duration() >= SESSION_COUNTS_SEC);
        if let Some(key) = key {
            let mut cache = self.reported.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() > 1000 {
                cache.clear();
            }
            cache.insert(key, result.clone());
        }
        Ok(result)
    }

    /// At the start: goes through the kept logs of the player. A first start
    /// finds its log filled, because the backups reach back for weeks.
    ///
    /// ⚠ Only once per log file: remembered are name and size; a file that
    /// grows (the running `Game.log` always does) is read again. No duplicates
    /// arise, `merge` takes care of that. ⚠⚠ No limit to the newest N logs: it
    /// would only shift the work to the second start.
    ///
    /// ⚠⚠ ALL logs go to the playtime and the statistics, not only the open ones:
    /// each has a read position of its own. ⚠⚠ The re-assessment of the saved
    /// log runs EVEN when nothing is new: what is still open is decided by the
    /// last sessions, three of them are enough.
    ///
    /// Returns (total, newly added).
    pub fn scan_backlog(&self) -> Result<(usize, usize), ScanError> {
        let paths = self.paths();
        let backups = paths.log_backups(None);
        let running = paths
            .game_folder()
            .map(|game| Flavor::native().join(&game, "Game.log"))
            .filter(|candidate| std::path::Path::new(candidate).is_file());
        let all: Vec<String> = backups.iter().cloned().chain(running).collect();

        let marks = load_read_marks(paths);
        let mut pending_files: Vec<(String, String)> = Vec::new();
        for path in &all {
            let Ok(meta) = fs::metadata(path) else { continue };
            let mark = meta.len().to_string();
            let Some(map) = &marks else {
                return Err(ScanError("Lesestand des Protokolls hat keinen brauchbaren Aufbau".into()));
            };
            if get(map, &Flavor::native().basename(path)) != Some(&Value::Str(mark.clone())) {
                pending_files.push((path.clone(), mark));
            }
        }

        playtime::catch_up(paths, &all);
        // Always: 5.0 has no switch for the statistics.
        play_stats::catch_up(self.src, &all, &*self.stats_clock);
        location::learn_all(paths, &all);
        stock_journal::catch_up(self.src, &all);

        let mut old = log::load_checked(paths).map_err(|e| ScanError(e.to_string()))?;
        let context = self.context();
        let open_now: Vec<usize> =
            (0..old.len()).filter(|&i| get(&old[i], "zustand") == Some(&Value::str(RUNNING))).collect();
        if !open_now.is_empty() {
            let candidates: Vec<String> = {
                let sorted = self.by_session(&all);
                sorted[sorted.len().saturating_sub(3)..].to_vec()
            };
            let (mut pending, mut done) = (open_now, Vec::new());
            for path in &candidates {
                let (reported, counts) = match self.reported_titles(path, &context.patterns) {
                    Ok(found) => found,
                    Err(e) => {
                        errors::record("mission_log.reassess", &e);
                        continue;
                    }
                };
                close_expired(
                    &mut Stored(&mut old),
                    &mut pending,
                    &mut done,
                    &reported,
                    &self.session_start(path),
                    counts,
                );
            }
            // ⚠ The file is written without `gelesen`, and with no new log the
            // function returns before the read position is written back: the
            // next start reads every log again.
            if !done.is_empty() {
                log::save(paths, &old);
            }
        }

        if pending_files.is_empty() {
            return Ok((old.len(), 0));
        }
        let known: HashSet<(String, String)> = old.iter().map(key_of).collect();
        // ⚠ In order of time, else a contract gets the end of a foreign run.
        let fresh = self.by_session(&pending_files.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>());
        let new = self.history_of(&fresh, &context)?;
        let merged = log::merge(&old, &new);
        let added = merged.iter().filter(|e| !known.contains(&key_of(e))).count();
        if log::save(paths, &merged)
            && let Some(mut map) = marks
        {
            for (path, mark) in &pending_files {
                map_set(&mut map, &Flavor::native().basename(path), Value::Str(mark.clone()));
            }
            save_read_marks(paths, map);
        }
        Ok((merged.len(), added))
    }

    /// Reads all logs once more, including the already known ones, and corrects
    /// the states (`reassess`). Returns (total, added, corrected).
    ///
    /// ⚠ A kept contract is never touched again by the backlog reading, so a
    /// better evaluation would reach only future ones. ⚠ Merged, not replaced:
    /// contracts from logs the game deleted long ago stand only here.
    pub fn reassess(
        &self,
        folder: Option<&str>,
        running_log: Option<&str>,
    ) -> Result<(usize, usize, usize), ScanError> {
        let paths = self.paths();
        let old = log::load_checked(paths).map_err(|e| ScanError(e.to_string()))?;
        let before: HashMap<(String, String), Option<Value>> =
            old.iter().map(|e| (key_of(e), get(e, "zustand").cloned())).collect();
        // ⚠ Not the folder itself: the kept sessions lie one level deeper, in `logbackups/`.
        let mut files: Vec<String> =
            folder.filter(|f| !f.is_empty()).map(|f| paths.log_backups(Some(f))).unwrap_or_default();
        if let Some(running) = running_log.filter(|r| std::path::Path::new(r).is_file()) {
            files.push(running.to_string());
        }
        // ⚠ Python sorts a set here: logs with the same first stamp come in an
        // order that changes from run to run. The order of the list is kept.
        let mut unique = HashSet::new();
        files.retain(|f| unique.insert(f.clone()));
        let new = if files.is_empty() {
            Vec::new()
        } else {
            let context = self.context();
            self.history_of(&self.by_session(&files), &context)?
        };
        if new.is_empty() {
            return Ok((old.len(), 0, 0));
        }
        let merged = log::merge(&old, &new);
        let (mut added, mut corrected) = (0, 0);
        for e in &merged {
            match before.get(&key_of(e)) {
                None => added += 1,
                Some(state) if !jsonio::py_eq(get(e, "zustand"), state.as_ref()) => corrected += 1,
                Some(_) => {}
            }
        }
        // ⚠ Written without `gelesen`: the next backlog reading starts from nothing.
        log::save(paths, &merged);
        Ok((merged.len(), added, corrected))
    }
}

/// The kept read position of the log file: name → size as text. Empty for an
/// old format or an unreadable file; `None` when `gelesen` is something Python
/// could not call `.get` on.
///
/// ⚠⚠ The read position must follow the format. Else a format change would
/// drop the old log while the position holds every log for done, and the
/// player stands before an EMPTY log without a word.
fn load_read_marks(paths: &Paths) -> Option<Map> {
    let Ok(Value::Object(map)) = jsonio::read_json(&paths.app_file(log::FILE)) else { return Some(Vec::new()) };
    if !log::format_is_current(&map) {
        return Some(Vec::new());
    }
    match get(&map, "gelesen") {
        Some(Value::Object(marks)) => Some(marks.clone()),
        Some(v) if log::truthy(v) => None,
        _ => Some(Vec::new()),
    }
}

/// Writes the read position next to the log, into the same file.
fn save_read_marks(paths: &Paths, marks: Map) {
    let file = paths.app_file(log::FILE);
    let result = match jsonio::read_json(&file) {
        Ok(Value::Object(mut data)) => {
            map_set(&mut data, "gelesen", Value::Object(marks));
            jsonio::save_json(&file, &Value::Object(data), 1, false).map_err(|e| e.to_string())
        }
        Ok(_) => Err("Protokoll ist kein Objekt".to_string()),
        Err(e) => Err(e.to_string()),
    };
    if let Err(message) = result {
        errors::record("mission_log.read_marks", &message);
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::legacy;
    use crate::paths::{ENV_INSTALL_DIR, Host};

    struct Lab {
        root: String,
        home: String,
        game: String,
    }

    impl Lab {
        fn new(name: &str) -> Lab {
            let f = Flavor::native();
            let base = std::env::temp_dir().to_string_lossy().into_owned();
            let root = f.join(&base, &format!("versekit-scan-{}-{name}", crate::testdir::run_tag()));
            let _ = fs::remove_dir_all(&root);
            let home = f.join(&root, "home");
            let game = f.join_all(&root, &["StarCitizen", "LIVE"]);
            fs::create_dir_all(&home).expect("Ordner");
            fs::create_dir_all(f.join(&game, "logbackups")).expect("Ordner");
            // Without a `Game.log` the folder is not taken for a game folder.
            fs::write(f.join(&game, "Game.log"), "").expect("Datei");
            Lab { root, home, game }
        }

        fn paths(&self) -> Paths {
            Paths::new(Host::new(
                Flavor::native(),
                &[(legacy::ENV_HOME, self.home.as_str()), (ENV_INSTALL_DIR, self.game.as_str())],
                &self.root,
            ))
        }

        fn backup(&self, name: &str, text: &str) -> String {
            let path = Flavor::native().join_all(&self.game, &["logbackups", name]);
            fs::write(&path, text).expect("Datei");
            path
        }
    }

    impl Drop for Lab {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn at(minute: u32) -> String {
        format!("2026-10-01T10:{:02}:{:02}", minute / 60 % 60, minute % 60)
    }

    fn notice(second: u32, phrase: &str, title: &str, mission: &str, objective: &str) -> String {
        format!(
            "<{}.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{phrase}: {title}: \" [1] MissionId: [{mission}], ObjectiveId: [{objective}]\n",
            at(second)
        )
    }

    fn accept(second: u32, title: &str, mission: &str) -> String {
        notice(second, "Auftrag angenommen", title, mission, "")
    }

    fn end(second: u32, title: &str, mission: &str, kind: &str) -> String {
        format!(
            "<{}.000Z> <EndMission> x MissionId[{mission}] Player[Xharig] CompletionType[{kind}]\n{}",
            at(second),
            notice(second, "Auftrag abgeschlossen", title, mission, "")
        )
    }

    fn blueprint(second: u32, name: &str) -> String {
        format!(
            "<{}.000Z> <SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: {name}: \" [1]\n",
            at(second)
        )
    }

    fn session(body: &str) -> String {
        format!("<2026-10-01T10:00:00.000Z> start OnClientSpawned\n{body}<2026-10-01T10:59:59.000Z> ende\n")
    }

    fn patterns() -> (Patterns, Regex) {
        (
            Patterns::of(&contracts::start_phrases(&[]), &contracts::end_phrases(&[])),
            crate::phrases::pattern(&["Bauplan erhalten".into()]),
        )
    }

    /// The history of the given texts as one run (the logs in the order given).
    fn history(texts: &[String]) -> Vec<Map> {
        let (patterns, blueprints) = patterns();
        let mut book = Book::default();
        for (i, text) in texts.iter().enumerate() {
            let (items, facts) = scan_text(text, &patterns, &blueprints);
            let session = session_start_of_text(text);
            let scan = FileScan { items, facts, session };
            let (reported, counts) = book.read(&scan, &format!("log{i}.log"));
            let Book { runs, pending, done, .. } = &mut book;
            close_expired(runs, pending, done, &reported, &scan.session, counts);
        }
        book.finish().iter().map(Run::to_map).collect()
    }

    fn session_start_of_text(text: &str) -> String {
        lines_universal(text).iter().take(200).map(|l| time_of(l)).find(|t| !t.is_empty()).unwrap_or("9999").to_string()
    }

    fn state_of(entry: &Map) -> &str {
        text_of(entry, "zustand")
    }

    fn by_name<'a>(entries: &'a [Map], name: &str) -> Vec<&'a Map> {
        entries.iter().filter(|e| text_of(e, "name") == name).collect()
    }

    #[test]
    fn four_outcomes_and_an_unknown_one_stays_completed() {
        assert_eq!(state_for("Complete"), COMPLETED);
        assert_eq!(state_for("Abandon"), ABORTED);
        assert_eq!(state_for("Fail"), FAILED);
        assert_eq!(state_for("Deactivate"), EXPIRED);
        assert_eq!(state_for("ABANDONED"), ABORTED);
        assert_eq!(state_for(""), COMPLETED);
        assert_eq!(state_for("Neues"), COMPLETED);
    }

    #[test]
    fn the_double_report_counts_once_and_a_resumption_is_no_new_run() {
        let body = format!(
            "{}{}{}{}",
            accept(1, "Retake Platforms", "m1"),
            accept(1, "Retake Platforms", "m2"),
            end(5, "Retake Platforms", "m1", "Complete"),
            accept(10, "Retake Platforms <EM4>[BP!]</EM4>", "m3"),
        );
        let entries = history(&[session(&body)]);
        let runs = by_name(&entries, "Retake Platforms");
        // Two acceptances in the same second are one; the second one at 10 is a run of its own.
        assert_eq!(runs.len(), 2);
        assert_eq!(state_of(runs[0]), RUNNING);
        assert_eq!(state_of(runs[1]), COMPLETED);
        assert_eq!(text_of(runs[1], "bis"), at(5).as_str());
    }

    #[test]
    fn logging_in_again_reports_a_running_contract_without_a_new_run() {
        let first = session(&accept(1, "Hunt", "m1"));
        let again = format!(
            "<2026-10-02T09:00:00.000Z> start OnClientSpawned\n{}{}<2026-10-02T09:59:59.000Z> ende\n",
            notice(1, "Auftrag angenommen", "Hunt", "m1", "").replace("2026-10-01", "2026-10-02"),
            end(30, "Hunt", "m1", "Complete").replace("2026-10-01", "2026-10-02"),
        );
        let entries = history(&[first, again]);
        let runs = by_name(&entries, "Hunt");
        assert_eq!(runs.len(), 1);
        assert_eq!(state_of(runs[0]), COMPLETED);
        // Accepted in the first session, ended in the second.
        assert!(text_of(runs[0], "wann").starts_with("2026-10-01"));
        assert!(text_of(runs[0], "bis").starts_with("2026-10-02"));
    }

    #[test]
    fn an_objective_ends_no_contract_and_the_mission_id_closes_a_renamed_one() {
        let body = format!(
            "{}{}{}",
            accept(1, "Hunt", "m1"),
            notice(2, "Auftrag zurückgezogen", "Obere Plattform", "m1", "o1"),
            // The end carries another title than the acceptance.
            end(3, "Hunt (beendet)", "m1", "Fail"),
        );
        let entries = history(&[session(&body)]);
        let runs = by_name(&entries, "Hunt");
        assert_eq!(runs.len(), 1);
        assert_eq!(state_of(runs[0]), FAILED);
        assert!(by_name(&entries, "Obere Plattform").is_empty());
        assert!(by_name(&entries, "Hunt (beendet)").is_empty());
    }

    #[test]
    fn the_silent_end_by_endmission_and_deactivate_expires() {
        let body = format!(
            "{}{}<{}.000Z> <EndMission> x MissionId[m1] Player[Xharig] CompletionType[Deactivate]\n",
            accept(1, "Hunt", "m1"),
            accept(2, "Rescue", "m2"),
            at(4)
        );
        let entries = history(&[session(&body)]);
        assert_eq!(state_of(by_name(&entries, "Hunt")[0]), EXPIRED);
        assert_eq!(state_of(by_name(&entries, "Rescue")[0]), RUNNING);
    }

    #[test]
    fn a_placeholder_acceptance_is_no_entry_and_leaving_the_world_clears_nothing() {
        let body = format!(
            "{}{}<{}.000Z> [CSessionManager::RequestFrontEnd] Started - Reason\n",
            accept(1, "Ling Family - Rang: ~mission(ReputationRank)", "m1"),
            accept(2, "Hunt", "m2"),
            at(3)
        );
        let entries = history(&[session(&body)]);
        assert_eq!(entries.len(), 1);
        assert_eq!(state_of(&entries[0]), RUNNING);
    }

    #[test]
    fn a_blueprint_goes_to_the_running_contract_first_and_only_one_per_contract() {
        let body = format!(
            "{}{}{}{}{}{}",
            accept(1, "A", "ma"),
            end(10, "A", "ma", "Complete"),
            accept(11, "B", "mb"),
            blueprint(12, "Helm"),
            blueprint(13, "Rucksack"),
            blueprint(14, "Rucksack"),
        );
        let entries = history(&[session(&body)]);
        // The running contract gets the first; A ended at 10 and is still in its grace time.
        assert_eq!(
            by_name(&entries, "B")[0].iter().find(|(k, _)| k == "bauplaene").map(|(_, v)| v.clone()),
            Some(Value::Array(vec![Value::str("Helm")]))
        );
        assert_eq!(
            by_name(&entries, "A")[0].iter().find(|(k, _)| k == "bauplaene").map(|(_, v)| v.clone()),
            Some(Value::Array(vec![Value::str("Rucksack")]))
        );
    }

    #[test]
    fn a_blueprint_after_the_grace_time_belongs_to_nobody() {
        let late = format!(
            "<2026-10-01T10:{:02}:00.000Z> <SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Helm: \" [1]\n",
            20
        );
        let body = format!("{}{}{late}", accept(1, "A", "ma"), end(10, "A", "ma", "Complete"));
        let entries = history(&[session(&body)]);
        assert_eq!(
            by_name(&entries, "A")[0].iter().find(|(k, _)| k == "bauplaene").map(|(_, v)| v.clone()),
            Some(Value::Array(vec![]))
        );
    }

    #[test]
    fn a_contract_a_later_session_does_not_name_expires() {
        let first = session(&format!("{}{}", accept(1, "Alt", "m1"), accept(2, "Bleibt", "m2")));
        let second = "<2026-10-02T09:00:00.000Z> start OnClientSpawned\n<2026-10-02T09:00:30.000Z> <SHUDEvent_OnNotification> Added notification \"Auftrag angenommen: Bleibt: \" [1] MissionId: [m2], ObjectiveId: []\n<2026-10-02T09:10:00.000Z> ende\n".to_string();
        let entries = history(&[first, second]);
        assert_eq!(state_of(by_name(&entries, "Alt")[0]), EXPIRED);
        assert_eq!(state_of(by_name(&entries, "Bleibt")[0]), RUNNING);
    }

    #[test]
    fn a_silent_session_proves_something_only_when_long_and_complete() {
        let first = session(&accept(1, "Alt", "m1"));
        let short = "<2026-10-02T09:00:00.000Z> start OnClientSpawned\n<2026-10-02T09:30:00.000Z> ende\n".to_string();
        let long = "<2026-10-02T09:00:00.000Z> start OnClientSpawned\n<2026-10-02T10:30:00.000Z> ende\n".to_string();
        let no_spawn = "<2026-10-02T09:00:00.000Z> start\n<2026-10-02T11:30:00.000Z> ende\n".to_string();
        assert_eq!(state_of(&history(&[first.clone(), short])[0]), RUNNING);
        assert_eq!(state_of(&history(&[first.clone(), no_spawn])[0]), RUNNING);
        // Exactly 90 minutes counts.
        assert_eq!(state_of(&history(&[first, long])[0]), EXPIRED);
    }

    #[test]
    fn progress_only_with_one_open_contract_and_one_mission() {
        let state = |objective: &str, s: &str| {
            format!(
                "<2026-10-01T10:00:02.000Z> <ObjectiveUpserted> x mission_id m1 - objective_id {objective} - state MISSION_OBJECTIVE_STATE_{s} - flags=ShowInLog\n"
            )
        };
        let body = format!(
            "{}{}{}{}{}",
            accept(1, "Hunt", "m1"),
            state("o1", "COMPLETED"),
            state("o2", "INPROGRESS"),
            state("phase_1", "COMPLETED"),
            state("o1", "COMPLETED"),
        );
        let entries = history(&[session(&body)]);
        assert_eq!(entries[0].iter().find(|(k, _)| k == "ziele_gesamt").map(|(_, v)| v.clone()), Some(Value::int(2)));
        assert_eq!(entries[0].iter().find(|(k, _)| k == "ziele_fertig").map(|(_, v)| v.clone()), Some(Value::int(1)));
        // Two open contracts: no guessing.
        let two = format!("{}{}{}", accept(1, "Hunt", "m1"), accept(2, "Rescue", "m2"), state("o1", "COMPLETED"));
        let entries = history(&[session(&two)]);
        assert!(entries.iter().all(|e| e.iter().any(|(k, v)| k == "ziele_gesamt" && *v == Value::int(0))));
    }

    #[test]
    fn same_start_keeps_the_order_of_the_runs() {
        let body = format!("{}{}", accept(1, "Erster", "m1"), accept(1, "Zweiter", "m2"));
        let entries = history(&[session(&body)]);
        // Stable and descending: equal times keep their order.
        let names: Vec<&str> = entries.iter().map(|e| text_of(e, "name")).collect();
        assert_eq!(names, ["Erster", "Zweiter"]);
    }

    #[test]
    fn the_head_of_a_log_gives_the_session_time() {
        let lab = Lab::new("kopf");
        let with = lab.backup("a.log", "ohne\nStempel\n<2026-10-01T10:00:00.5Z> x\n");
        let without = lab.backup("b.log", "gar keiner\n");
        assert_eq!(session_start_of(&with), "2026-10-01T10:00:00");
        assert_eq!(session_start_of(&without), "9999");
        assert_eq!(session_start_of(&Flavor::native().join(&lab.game, "gibtsnicht.log")), "9999");
        // A stamp only after the 200th line does not count.
        let late = format!("{}<2026-10-01T10:00:00.5Z>\n", "zeile\n".repeat(200));
        assert_eq!(session_start_of(&lab.backup("c.log", &late)), "9999");
        let in_time = format!("{}<2026-10-01T10:00:00.5Z>\n", "zeile\n".repeat(199));
        assert_eq!(session_start_of(&lab.backup("d.log", &in_time)), "2026-10-01T10:00:00");
    }

    #[test]
    fn scanning_twice_reads_nothing_new_and_a_grown_log_is_read_again() {
        let lab = Lab::new("lauf");
        lab.backup("a.log", &session(&format!("{}{}", accept(1, "Hunt", "m1"), end(5, "Hunt", "m1", "Complete"))));
        let paths = lab.paths();
        let src = LogSource::new(&paths, LogSource::sources_of(&paths));
        let scan = MissionScan::new(&src);
        assert_eq!(scan.scan_backlog(), Ok((1, 1)));
        assert_eq!(scan.scan_backlog(), Ok((1, 0)));
        let saved = log::load(&paths);
        assert_eq!(saved.len(), 1);
        assert_eq!(state_of(&saved[0]), COMPLETED);
        // The file holds the read position next to the contracts, and the sessions are kept.
        let text = fs::read_to_string(paths.app_file(log::FILE)).expect("Datei");
        assert!(text.contains("\"gelesen\""));
        assert!(Path::new(&paths.app_file(playtime::FILE)).is_file());
        assert!(Path::new(&paths.app_file(play_stats::FILE)).is_file());
        // A grown file counts as unread.
        lab.backup(
            "a.log",
            &session(&format!(
                "{}{}{}",
                accept(1, "Hunt", "m1"),
                end(5, "Hunt", "m1", "Complete"),
                accept(9, "Neu", "m9")
            )),
        );
        assert_eq!(scan.scan_backlog(), Ok((2, 1)));
    }

    /// The real logs of an installation (`VERSEKIT_SPIELORDNER`, read only), a
    /// throwaway data folder: how long a first reading and a repeated one take.
    #[test]
    #[ignore = "needs the logs of an installation"]
    fn real_logs_timing() {
        let Ok(game) = std::env::var("VERSEKIT_SPIELORDNER") else { return };
        let lab = Lab::new("zeit");
        crate::writeguard::deny(true);
        crate::writeguard::allow_under(Some(&lab.root));
        let paths = Paths::new(Host::new(
            Flavor::native(),
            &[(legacy::ENV_HOME, lab.home.as_str()), (ENV_INSTALL_DIR, game.as_str())],
            &lab.root,
        ));
        let src = LogSource::new(&paths, LogSource::sources_of(&paths));
        let scan = MissionScan::new(&src);
        for round in ["first reading", "second reading"] {
            let started = std::time::Instant::now();
            let result = scan.scan_backlog();
            println!("{round}: {result:?} in {:.2?}", started.elapsed());
        }
        println!("not written (guard): {:?}", crate::writeguard::take_denied());
        crate::writeguard::deny(false);
        crate::writeguard::allow_under(None);
    }

    #[test]
    fn a_log_that_cannot_be_read_stops_the_history_and_writes_no_contract_log() {
        let lab = Lab::new("kaputt");
        let paths = lab.paths();
        let src = LogSource::new(&paths, LogSource::sources_of(&paths));
        let scan = MissionScan::new(&src);
        let (patterns, blueprints) = patterns();
        let context = Context { patterns, blueprints };
        let missing = Flavor::native().join(&lab.game, "weg.log");
        assert!(scan.history_of(&[missing], &context).is_err());
        assert!(!Path::new(&paths.app_file(log::FILE)).exists());
    }
}
