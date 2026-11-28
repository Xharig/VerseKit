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

//! The flight log: what happened in each session, in the order it happened (`flugbuch.json`). Every log is read
//! once and its events kept per session; the contracts and the blueprints found come from the contract log
//! (`mission_log`) when the log is shown and are not read again.
//!
//! ⚠ The flight log is a profile of movement (where, when, in which ship). It stays on this computer and nothing
//! sends it anywhere. Only named fields are written (see [`Data`]); no line of the log is ever kept as text, and
//! the address and the port of a server, ids of any kind and the handles of other players have no field to land
//! in.
//!
//! ⚠ Two things are guesses: a death is told only by the body that stays behind (the rule of the statistics,
//! [`play_stats::Deaths`]), and an arrival counts only for a ship the player chose a target in or left (the log
//! names the arrival of every ship nearby).
//!
//! The file is its own: it shares no key with `statistik.json` and changes none of its numbers.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::LazyLock;

use regex::Regex;

use crate::injuries::{self, Phrases};
use crate::jsonio::{self, Map, Value, map_set};
use crate::language::{Lang, t_in, tf_in};
use crate::location;
use crate::logread;
use crate::logsource::LogSource;
use crate::logtime::{FactsScan, candidate_lines, seconds, time_of};
use crate::mission_log::{self, get};
use crate::ordered::parallel_map;
use crate::paths::Paths;
use crate::play_stats::{self, Deaths};
use crate::playtime::{number, write_beside};
use crate::pypath::Flavor;
use crate::stats_report;
use crate::text::decode_replace;

pub const FILE: &str = "flugbuch.json";
pub const FORMAT: i64 = 1;
/// The most events one session keeps; the ones that come often go first (see [`Kind::rank`]).
pub const MAX_EVENTS: usize = 5000;
/// A lost connection is told by two lines in the same moment; two within this many seconds are one event.
const CONNECTION_GAP: i64 = 5;
/// The code of a connection that timed out.
const TIMED_OUT: &str = "30000";

const QUIT: &str = "<SystemQuit>";
const QUANTUM: &str = "<Quantum Drive Arrived - Arrived at Final Destination>";
const CRASH: [&str; 2] = ["crash handler taking over", "-- GPU CRASH"];
const TARGET_MARK: &str = "<Player Selected Quantum Target - Local>";
const ROUTE_MARK: &str = "<Calculate Route>";
const PLACE_MARK: &str = "<Update Inventory Location>";
const PLANET_MARK: &str = "<Planet CPU Maps Loaded>";
const LOADING_MARK: &str = "Loading screen for ";
const DISCONNECT_MARK: &str = "<Channel Process Disconnection>";
const POPUP_MARK: &str = "<Error Popup Opened>";
const SERVER_MARK: &str = "<Change Server Start>";
const CONTROL_MARK: &str = "<Vehicle Control Flow>";
const ATTACHMENT_MARK: &str = "<AttachmentReceived>";
const CORPSE_MARK: &str = "CSCActorCorpseUtils";
/// The log calls a light disconnect on every join; it is no loss of the connection.
const LIGHT_DISCONNECT: &str = "DisconnectLight";

/// The ship of a line of the log: `| RSI_Polaris_465232524500[1234567]|` (name with serial number, then its id).
static SHIP_BAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\|\s*([^\s|\[\]]+)\[\d+\]\s*\|").expect("fester Ausdruck"));
static PLANET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"{}.*?Planet '([^']+)' \(entity (\d+)\)", regex::escape(PLANET_MARK))).expect("fester Ausdruck")
});
static LOADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Loading screen for ([A-Za-z0-9_]+) : [A-Za-z0-9_]+ closed after ([0-9]+(?:\.[0-9]+)?) seconds")
        .expect("fester Ausdruck")
});
static CODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:cause|errorCode)=(\d+)").expect("fester Ausdruck"));

/// The lines worth a look; every other line cannot change anything.
static MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    let literals = [
        QUIT,
        QUANTUM,
        CRASH[0],
        CRASH[1],
        TARGET_MARK,
        ROUTE_MARK,
        PLACE_MARK,
        PLANET_MARK,
        LOADING_MARK,
        DISCONNECT_MARK,
        POPUP_MARK,
        SERVER_MARK,
        CONTROL_MARK,
        ATTACHMENT_MARK,
        CORPSE_MARK,
        crate::shard::JOIN_MARK,
        injuries::NOTIFICATION,
    ];
    let escaped: Vec<String> = literals.iter().map(|l| regex::escape(l)).collect();
    Regex::new(&escaped.join("|")).expect("aus maskierten Teilen gebaut")
});

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Start,
    End,
    Crash,
    Shard,
    ServerChange,
    Place,
    Target,
    Jump,
    Death,
    Injury,
    Connection,
    Loading,
    Ship,
}

impl Kind {
    pub const ALL: [Kind; 13] = [
        Kind::Start,
        Kind::End,
        Kind::Crash,
        Kind::Shard,
        Kind::ServerChange,
        Kind::Place,
        Kind::Target,
        Kind::Jump,
        Kind::Death,
        Kind::Injury,
        Kind::Connection,
        Kind::Loading,
        Kind::Ship,
    ];

    /// The name in the file.
    pub fn stored(self) -> &'static str {
        match self {
            Kind::Start => "start",
            Kind::End => "ende",
            Kind::Crash => "absturz",
            Kind::Shard => "shard",
            Kind::ServerChange => "serverwechsel",
            Kind::Place => "ort",
            Kind::Target => "ziel",
            Kind::Jump => "sprung",
            Kind::Death => "tod",
            Kind::Injury => "verletzung",
            Kind::Connection => "verbindung",
            Kind::Loading => "ladebild",
            Kind::Ship => "schiff",
        }
    }

    pub fn from_stored(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.stored() == name)
    }

    /// How much an event is worth keeping when a session has too many: the lower the rank, the sooner it goes.
    fn rank(self) -> u8 {
        match self {
            Kind::Loading => 0,
            Kind::Ship => 1,
            Kind::Place => 2,
            Kind::Target => 3,
            Kind::Jump => 4,
            Kind::Shard | Kind::ServerChange => 5,
            Kind::Connection | Kind::Injury => 6,
            Kind::Start | Kind::End | Kind::Crash | Kind::Death => 7,
        }
    }
}

/// The fields an event may carry: the whole list of what the file keeps of a line of the log.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Data {
    /// The number of the place the player is at now (`ort`); `0` is on the way, at no place.
    pub place: Option<String>,
    /// The key of the target of a quantum jump (`ziel`); `None` where the log names it by an unknown number.
    pub target: Option<String>,
    /// The place the route was calculated from (`von`), a key.
    pub from: Option<String>,
    /// The class of the ship (`schiff`), without its serial number.
    pub ship: Option<String>,
    /// The shard that was joined (`shard`).
    pub shard: Option<String>,
    /// How bad an injury is (`stufe`): `leicht`, `mittel` or `schwer`.
    pub severity: Option<String>,
    /// The level a loading screen was for (`name`).
    pub name: Option<String>,
    /// How long it took, in seconds (`dauer`).
    pub seconds: Option<i64>,
}

/// One event of a session: when (seconds since 1970, UTC), what, and its named fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub at: i64,
    pub kind: Kind,
    pub data: Data,
}

impl Event {
    fn new(at: i64, kind: Kind) -> Event {
        Event { at, kind, data: Data::default() }
    }

    fn to_value(&self) -> Value {
        let mut map: Map = vec![("wann".into(), Value::int(self.at)), ("art".into(), Value::str(self.kind.stored()))];
        let d = &self.data;
        for (key, value) in [
            ("ort", &d.place),
            ("ziel", &d.target),
            ("von", &d.from),
            ("schiff", &d.ship),
            ("shard", &d.shard),
            ("stufe", &d.severity),
            ("name", &d.name),
        ] {
            if let Some(text) = value {
                map.push((key.into(), Value::str(text.as_str())));
            }
        }
        if let Some(seconds) = d.seconds {
            map.push(("dauer".into(), Value::int(seconds)));
        }
        Value::Object(map)
    }

    /// The event of a stored value; one of a kind this version does not know is skipped.
    fn from_value(value: &Value) -> Option<Event> {
        let Value::Object(map) = value else { return None };
        let at = number(get(map, "wann"))? as i64;
        let Some(Value::Str(name)) = get(map, "art") else { return None };
        let kind = Kind::from_stored(name)?;
        let text = |key: &str| match get(map, key) {
            Some(Value::Str(s)) => Some(s.clone()),
            _ => None,
        };
        let data = Data {
            place: text("ort"),
            target: text("ziel"),
            from: text("von"),
            ship: text("schiff"),
            shard: text("shard"),
            severity: text("stufe"),
            name: text("name"),
            seconds: number(get(map, "dauer")).map(|s| s as i64),
        };
        Some(Event { at, kind, data })
    }
}

/// One session: the log of one game start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    /// The first stamp of the log (seconds, UTC): the key of the session, the same as in `statistik.json`.
    pub start: i64,
    pub end: i64,
    pub account: String,
    pub events: Vec<Event>,
}

impl Session {
    fn to_map(&self) -> Map {
        vec![
            ("von".into(), Value::int(self.start)),
            ("bis".into(), Value::int(self.end)),
            ("account".into(), Value::str(self.account.as_str())),
            ("ereignisse".into(), Value::Array(self.events.iter().map(Event::to_value).collect())),
        ]
    }

    fn from_map(map: &Map) -> Option<Session> {
        let start = number(get(map, "von"))? as i64;
        let end = number(get(map, "bis"))? as i64;
        let account = match get(map, "account") {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let events = match get(map, "ereignisse") {
            Some(Value::Array(items)) => items.iter().filter_map(Event::from_value).collect(),
            _ => Vec::new(),
        };
        Some(Session { start, end, account, events })
    }

    /// How many events of `kind` the session holds.
    pub fn count(&self, kind: Kind) -> usize {
        self.events.iter().filter(|e| e.kind == kind).count()
    }
}

/// Reads a log a piece at a time and writes down its events.
pub struct Scan<'a> {
    account: Option<&'a str>,
    phrases: &'a Phrases,
    facts: FactsScan,
    events: Vec<Event>,
    /// The ships of the player: the names (with serial number) of the ships a target was chosen in or a seat was left.
    own_ships: HashSet<String>,
    /// Planet name by the number of its entity, of this session.
    planets: HashMap<String, String>,
    last_target: Option<String>,
    start_pending: bool,
    deaths: Deaths,
    hurt: HashMap<String, i64>,
    last_connection: Option<i64>,
    ended: bool,
    crashed: bool,
}

impl<'a> Scan<'a> {
    pub fn new(account: Option<&'a str>, phrases: &'a Phrases) -> Scan<'a> {
        Scan {
            account: account.filter(|a| !a.is_empty()),
            phrases,
            facts: FactsScan::default(),
            events: Vec::new(),
            own_ships: HashSet::new(),
            planets: HashMap::new(),
            last_target: None,
            start_pending: false,
            deaths: Deaths::default(),
            hurt: HashMap::new(),
            last_connection: None,
            ended: false,
            crashed: false,
        }
    }

    /// The next piece of the log: whole lines.
    pub fn piece(&mut self, text: &str) {
        self.facts.piece(text);
        for line in candidate_lines(text, &MARKERS) {
            self.line(line);
        }
    }

    fn push(&mut self, event: Event) {
        self.events.push(event);
        if self.events.len() > 2 * MAX_EVENTS {
            cap(&mut self.events);
        }
    }

    /// The target of a choice: a key, or the name of a planet where the log names it by the number of its entity;
    /// `None` for a number nothing is known about.
    fn target_of(&self, raw: &str) -> Option<String> {
        if raw.bytes().all(|b| b.is_ascii_digit()) {
            return self.planets.get(raw).cloned();
        }
        Some(play_stats::place_key(raw))
    }

    #[allow(clippy::too_many_lines)]
    fn line(&mut self, line: &str) {
        // ⚠ A line without a stamp tells nothing about when: it is passed over.
        let stamp = Some(time_of(line)).filter(|s| !s.is_empty());
        let Some(at) = stamp.and_then(seconds).filter(|s| *s != 0) else { return };
        if line.contains(QUIT) {
            if !self.ended {
                self.ended = true;
                self.push(Event::new(at, Kind::End));
            }
        } else if line.contains(QUANTUM) {
            if let Some(c) = SHIP_BAR.captures(line)
                && self.own_ships.contains(&c[1])
            {
                let mut event = Event::new(at, Kind::Jump);
                event.data.ship = Some(play_stats::ship_class(&c[1]));
                event.data.target.clone_from(&self.last_target);
                self.push(event);
            }
        } else if line.contains(ATTACHMENT_MARK) {
            let account = self.account.map(str::to_lowercase);
            let own = |player: &str| account.as_deref().is_none_or(|a| player.to_lowercase() == a);
            if let Some(number) = play_stats::own_body_in(line, &own) {
                self.deaths.body(&number);
            }
        } else if line.contains(CORPSE_MARK) {
            if let Some(moment) = self.deaths.corpse(line, stamp) {
                self.push(Event::new(moment, Kind::Death));
            }
        } else if line.contains(CONTROL_MARK) {
            if let Some(c) = play_stats::CONTROL.captures(line) {
                self.own_ships.insert(c[1].to_string());
                let mut event = Event::new(at, Kind::Ship);
                event.data.ship = Some(play_stats::ship_class(&c[1]));
                self.push(event);
            }
        } else if line.contains(TARGET_MARK) {
            if let Some(c) = play_stats::TARGET.captures(line) {
                if let Some(ship) = SHIP_BAR.captures(line) {
                    self.own_ships.insert(ship[1].to_string());
                }
                let target = self.target_of(&c[1]);
                self.start_pending = true;
                self.last_target.clone_from(&target);
                let mut event = Event::new(at, Kind::Target);
                event.data.target = target;
                self.push(event);
            }
        } else if self.start_pending && line.contains(ROUTE_MARK) {
            // ⚠ Only the first route calculation after choosing a target: the game often calculates it again.
            if let Some(c) = play_stats::START.captures(line) {
                self.start_pending = false;
                if let Some(event) = self.events.iter_mut().rev().find(|e| e.kind == Kind::Target) {
                    event.data.from = Some(crate::text::py_strip(&c[1]).to_string());
                }
            }
        } else if line.contains(DISCONNECT_MARK) || line.contains(POPUP_MARK) {
            let timed_out = !line.contains(LIGHT_DISCONNECT) && CODE.captures(line).is_some_and(|c| &c[1] == TIMED_OUT);
            if timed_out && self.last_connection.is_none_or(|before| at - before > CONNECTION_GAP) {
                self.last_connection = Some(at);
                self.push(Event::new(at, Kind::Connection));
            }
        } else if line.contains(SERVER_MARK) {
            self.push(Event::new(at, Kind::ServerChange));
        } else if line.contains(crate::shard::JOIN_MARK) {
            if let Some(join) = crate::shard::join_in(line) {
                let mut event = Event::new(join.at.unwrap_or(at), Kind::Shard);
                event.data.shard = Some(join.shard.id);
                self.push(event);
            }
        } else if line.contains(PLACE_MARK) {
            if let Some((before, now)) = location::change_in(line)
                && before != now
            {
                let mut event = Event::new(at, Kind::Place);
                event.data.place = Some(now);
                self.push(event);
            }
        } else if line.contains(PLANET_MARK) {
            if let Some(c) = PLANET.captures(line) {
                self.planets.insert(c[2].to_string(), c[1].to_string());
            }
        } else if line.contains(LOADING_MARK) {
            if let Some(c) = LOADING.captures(line) {
                let mut event = Event::new(at, Kind::Loading);
                event.data.name = Some(c[1].to_string());
                event.data.seconds = c[2].parse::<f64>().ok().map(|s| s.round() as i64);
                self.push(event);
            }
        } else if line.contains(injuries::NOTIFICATION) {
            if let Some((severity, text)) = self.phrases.notice(line) {
                let again = self.hurt.get(text).is_some_and(|before| at - before <= injuries::REPEAT_GAP);
                self.hurt.insert(text.to_string(), at);
                if !again {
                    let mut event = Event::new(at, Kind::Injury);
                    event.data.severity = Some(severity.stored().to_string());
                    self.push(event);
                }
            }
        } else if !self.crashed && CRASH.iter().any(|mark| line.contains(mark)) {
            self.crashed = true;
            self.push(Event::new(at, Kind::Crash));
        }
    }

    /// The session, or `None` when the player never arrived or the stamps make no session.
    pub fn finish(mut self) -> Option<Session> {
        let facts = self.facts.facts().clone();
        if !facts.spawn {
            return None;
        }
        let start = seconds(facts.first.as_deref().unwrap_or("")).filter(|s| *s != 0)?;
        let end = seconds(facts.last.as_deref().unwrap_or("")).filter(|s| *s != 0)?;
        if end < start {
            return None;
        }
        if let Some(spawned) = seconds(facts.spawned.as_deref().unwrap_or("")).filter(|s| *s != 0) {
            self.events.insert(0, Event::new(spawned, Kind::Start));
        }
        // Stable: events of one second stay in the order of the log.
        self.events.sort_by_key(|e| e.at);
        cap(&mut self.events);
        Some(Session { start, end, account: self.account.unwrap_or("").to_string(), events: self.events })
    }
}

/// Keeps at most [`MAX_EVENTS`]: of the kinds that come often the oldest go first, the rare ones (start, end,
/// crash, death) stay.
fn cap(events: &mut Vec<Event>) {
    let mut excess = events.len().saturating_sub(MAX_EVENTS);
    for rank in 0..=7u8 {
        if excess == 0 {
            break;
        }
        events.retain(|e| {
            if excess > 0 && e.kind.rank() == rank {
                excess -= 1;
                false
            } else {
                true
            }
        });
    }
}

/// The session of one log's text.
pub fn read_log_text(text: &str, account: Option<&str>, phrases: &Phrases) -> Option<Session> {
    let mut scan = Scan::new(account, phrases);
    scan.piece(text);
    scan.finish()
}

/// [`read_log_text`] of a log file; an unreadable one has none.
///
/// ⚠ The file is read a piece at a time and the lines worth a look are turned into events at once, so a log of
/// gigabytes never stands in memory whole.
pub fn read_log(path: &str, account: Option<&str>, phrases: &Phrases) -> Option<Session> {
    let mut scan = Scan::new(account, phrases);
    logread::read_all(path, |bytes| scan.piece(&decode_replace(bytes))).ok()?;
    scan.finish()
}

/// The class of the last ship in `text` the player took or left the controls of, or chose a quantum target from;
/// `None` when the text names none.
pub fn last_ship_in(text: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        let name = if line.contains(CONTROL_MARK) {
            play_stats::CONTROL.captures(line)?.get(1)
        } else if line.contains(TARGET_MARK) {
            SHIP_BAR.captures(line)?.get(1)
        } else {
            None
        };
        name.map(|n| play_stats::ship_class(n.as_str()))
    })
}

fn empty_file() -> Map {
    vec![
        ("format".into(), Value::int(FORMAT)),
        ("sitzungen".into(), Value::Object(Vec::new())),
        ("gelesen".into(), Value::Object(Vec::new())),
    ]
}

/// `{'format':…, 'sitzungen': {von: {…}}, 'gelesen': {name: size}}`.
///
/// ⚠ An older format keeps its sessions but loses the read position: every log still there is read once more,
/// and what is already gone keeps what it has.
pub fn load(paths: &Paths) -> Map {
    let valid = |d: &Value| matches!(d, Value::Object(_));
    match jsonio::load_json(&paths.app_file(FILE), Some(&valid), "flugbuch.load") {
        Some(Value::Object(mut map)) if matches!(get(&map, "sitzungen"), Some(Value::Object(_))) => {
            if number(get(&map, "format")) != Some(FORMAT as f64) {
                map_set(&mut map, "gelesen", Value::Object(Vec::new()));
            }
            if get(&map, "gelesen").is_none() {
                map.push(("gelesen".into(), Value::Object(Vec::new())));
            }
            map
        }
        _ => empty_file(),
    }
}

/// Writes first beside, then renames; reports a failure.
pub fn save(paths: &Paths, data: &mut Map) -> bool {
    write_beside(paths, FILE, data, FORMAT, "flugbuch.save")
}

/// The sessions of the file, the oldest first.
pub fn sessions(paths: &Paths) -> Vec<Session> {
    let data = load(paths);
    let Some(Value::Object(map)) = get(&data, "sitzungen") else { return Vec::new() };
    let mut found: Vec<Session> = map
        .iter()
        .filter_map(|(_, value)| match value {
            Value::Object(entry) => Session::from_map(entry),
            _ => None,
        })
        .collect();
    found.sort_by_key(|s| s.start);
    found
}

/// Reads the logs that are new or have grown and writes the sessions on; returns the number of newly recorded
/// ones.
///
/// ⚠ Its own read position (name and size). ⚠ The key is the start of the session, not the file name: the running
/// `Game.log` is renamed at the next start and would count a second time.
pub fn catch_up(src: &LogSource<'_>, files: &[String]) -> usize {
    let paths = src.paths;
    let known = match get(&load(paths), "gelesen") {
        Some(Value::Object(map)) => map.clone(),
        _ => Vec::new(),
    };
    let mut marks: Vec<(String, i64)> = Vec::new();
    let mut jobs: Vec<(&String, Option<String>)> = Vec::new();
    for path in files {
        let Ok(meta) = fs::metadata(path) else { continue };
        let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
        let name = Flavor::native().basename(path);
        if number(get(&known, &name)) == Some(size as f64) {
            continue;
        }
        marks.push((name, size));
        jobs.push((path, src.account_of_file(path)));
    }
    if jobs.is_empty() {
        return 0;
    }
    let phrases = Phrases::of(&LogSource::sources_of(paths));
    let found: Vec<Session> =
        parallel_map(&jobs, |(path, account)| read_log(path, account.as_deref(), &phrases)).into_iter().flatten().collect();
    // ⚠ The reading above takes long and runs without the lock; what is written is loaded anew under it, or two
    // writers would overwrite each other's sessions.
    let _guard = jsonio::lock_saves();
    let mut data = load(paths);
    let mut kept: Map = match get(&data, "sitzungen") {
        Some(Value::Object(map)) => map.clone(),
        _ => Vec::new(),
    };
    let mut read: Map = match get(&data, "gelesen") {
        Some(Value::Object(map)) => map.clone(),
        _ => Vec::new(),
    };
    for (name, size) in &marks {
        map_set(&mut read, name, Value::int(*size));
    }
    let mut added = 0;
    for session in found {
        let key = session.start.to_string();
        if get(&kept, &key).is_none() {
            added += 1;
        }
        map_set(&mut kept, &key, Value::Object(session.to_map()));
    }
    map_set(&mut data, "sitzungen", Value::Object(kept));
    map_set(&mut data, "gelesen", Value::Object(read));
    save(paths, &mut data);
    added
}

/// "Read everything anew": only the read position is dropped, not the sessions (what has vanished from the logs
/// keeps its events).
pub fn rescan(src: &LogSource<'_>, files: &[String]) -> usize {
    {
        let _guard = jsonio::lock_saves();
        let mut data = load(src.paths);
        map_set(&mut data, "gelesen", Value::Object(Vec::new()));
        save(src.paths, &mut data);
    }
    catch_up(src, files)
}

// ---------------------------------------------------------------------------
// What the page shows
// ---------------------------------------------------------------------------

/// Where the player was at a moment of a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Where {
    /// No change of place stands in the session before this moment.
    Unknown,
    /// At no place: in flight or out in the open.
    Underway,
    /// At a place; its readable name, `None` while the book has not learned the number.
    At(Option<String>),
}

/// One line of the timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Event(Event),
    /// A contract accepted.
    Accepted { name: String },
    /// A contract ended, with its state (`mission_log::COMPLETED` and the like).
    Ended { name: String, state: String },
    /// A blueprint the contract gave.
    Find { name: String, contract: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub at: i64,
    pub item: Item,
    pub place: Where,
}

/// What a switch of the timeline shows or hides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    Session,
    Place,
    Target,
    Jump,
    Contract,
    Find,
    Death,
    Injury,
    Connection,
    Shard,
    Ship,
    Loading,
}

impl Topic {
    pub const ALL: [Topic; 12] = [
        Topic::Session,
        Topic::Place,
        Topic::Target,
        Topic::Jump,
        Topic::Contract,
        Topic::Find,
        Topic::Death,
        Topic::Injury,
        Topic::Connection,
        Topic::Shard,
        Topic::Ship,
        Topic::Loading,
    ];

    /// Shown from the start: all but the ones that come in numbers and tell little.
    pub fn on_at_start(self) -> bool {
        !matches!(self, Topic::Ship | Topic::Loading)
    }

    /// The topics an export may carry; a ship or a loading screen has no line in it.
    pub fn exportable(self) -> bool {
        !matches!(self, Topic::Ship | Topic::Loading)
    }
}

impl Row {
    pub fn topic(&self) -> Topic {
        match &self.item {
            Item::Accepted { .. } | Item::Ended { .. } => Topic::Contract,
            Item::Find { .. } => Topic::Find,
            Item::Event(event) => match event.kind {
                Kind::Start | Kind::End | Kind::Crash => Topic::Session,
                Kind::Shard | Kind::ServerChange => Topic::Shard,
                Kind::Place => Topic::Place,
                Kind::Target => Topic::Target,
                Kind::Jump => Topic::Jump,
                Kind::Death => Topic::Death,
                Kind::Injury => Topic::Injury,
                Kind::Connection => Topic::Connection,
                Kind::Loading => Topic::Loading,
                Kind::Ship => Topic::Ship,
            },
        }
    }
}

/// The numbers of one session for the tiles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Seconds from the first to the last stamp of the log.
    pub duration: i64,
    /// Own quantum jumps.
    pub jumps: usize,
    pub contracts: usize,
    pub finds: usize,
    pub deaths: usize,
    pub server_changes: usize,
    pub places: usize,
}

/// How often the shard changed in a session: a join on another shard than the one before. The first join is none.
fn shard_changes(session: &Session) -> usize {
    let ids: Vec<&String> =
        session.events.iter().filter(|e| e.kind == Kind::Shard).filter_map(|e| e.data.shard.as_ref()).collect();
    ids.windows(2).filter(|pair| pair[0] != pair[1]).count()
}

/// Everything the page needs, loaded once: the sessions, the contracts and the names of the places.
#[derive(Clone, Debug, Default)]
pub struct Book {
    pub sessions: Vec<Session>,
    contracts: Vec<mission_log::Mission>,
    learned: HashMap<String, String>,
    names: HashMap<String, String>,
    /// The names of ships and weapons, and of the parts of the places, as the statistics read them.
    things: stats_report::Names,
    places: stats_report::Names,
}

impl Book {
    /// A book of these sessions only: no contracts, no names (for tests and the sample page).
    pub fn of_sessions(sessions: Vec<Session>) -> Book {
        Book { sessions, ..Book::default() }
    }

    pub fn load(paths: &Paths) -> Book {
        Book {
            sessions: sessions(paths),
            contracts: mission_log::load(paths).iter().map(mission_log::Mission::of).collect(),
            learned: location::learned(paths),
            names: location::names(paths),
            things: stats_report::saved_names(paths, stats_report::NAME_FILE),
            places: stats_report::saved_names(paths, stats_report::PLACE_FILE),
        }
    }

    /// The readable name of a place number; `None` for a number that was never learned.
    pub fn place_name(&self, number: &str) -> Option<String> {
        self.learned.get(number).map(|key| location::display_name(key, &self.names))
    }

    /// The readable name of a target the player chose, in `lang`; `None` for a target the log named by an unknown
    /// number.
    pub fn target_name(&self, lang: Lang, key: Option<&str>) -> Option<String> {
        let place = stats_report::place_parts(key?, &self.places);
        Some(match place.text_key {
            Some(text) if place.name.is_empty() => t_in(lang, text),
            Some(text) => tf_in(lang, text, &[("0", place.name.as_str().into())]),
            None => place.name,
        })
    }

    /// The readable name of a ship class.
    pub fn ship_name(&self, class: &str) -> String {
        stats_report::display_name(class, true, &self.things)
    }

    /// The contracts that began while the session was on.
    fn contracts_of(&self, session: &Session) -> impl Iterator<Item = (&mission_log::Mission, i64)> {
        self.contracts.iter().filter_map(move |m| {
            let at = seconds(&m.when).filter(|s| *s != 0)?;
            (session.start..=session.end).contains(&at).then_some((m, at))
        })
    }

    /// The timeline of a session: its events and its contracts and finds, by time (what stands at the same moment
    /// keeps its order: events first).
    pub fn rows(&self, session: &Session) -> Vec<Row> {
        let mut rows: Vec<(i64, Item)> = session.events.iter().map(|e| (e.at, Item::Event(e.clone()))).collect();
        for (mission, at) in self.contracts_of(session) {
            rows.push((at, Item::Accepted { name: mission.name.clone() }));
            let ended = seconds(&mission.until).filter(|s| *s != 0);
            if mission.state != mission_log::RUNNING
                && let Some(end) = ended
            {
                rows.push((end, Item::Ended { name: mission.name.clone(), state: mission.state.clone() }));
            }
            for blueprint in &mission.blueprints {
                rows.push((ended.unwrap_or(at), Item::Find { name: blueprint.clone(), contract: mission.name.clone() }));
            }
        }
        rows.sort_by_key(|(at, _)| *at);
        let mut place = Where::Unknown;
        rows.into_iter()
            .map(|(at, item)| {
                if let Item::Event(Event { kind: Kind::Place, data, .. }) = &item
                    && let Some(number) = &data.place
                {
                    place = if number == location::NOWHERE { Where::Underway } else { Where::At(self.place_name(number)) };
                }
                Row { at, item, place: place.clone() }
            })
            .collect()
    }

    pub fn summary(&self, session: &Session) -> Summary {
        let mut contracts = 0;
        let mut finds = 0;
        for (mission, _) in self.contracts_of(session) {
            contracts += 1;
            finds += mission.blueprints.len();
        }
        Summary {
            duration: session.end - session.start,
            jumps: session.count(Kind::Jump),
            contracts,
            finds,
            deaths: session.count(Kind::Death),
            server_changes: session.count(Kind::ServerChange) + shard_changes(session),
            places: session.count(Kind::Place),
        }
    }

    /// The ship a session was flown in most: the class with the most events of its own.
    pub fn main_ship(&self, session: &Session) -> Option<String> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for ship in session.events.iter().filter(|e| matches!(e.kind, Kind::Jump | Kind::Ship)).filter_map(|e| e.data.ship.as_ref()) {
            match counts.iter_mut().find(|(class, _)| class == ship) {
                Some((_, n)) => *n += 1,
                None => counts.push((ship.clone(), 1)),
            }
        }
        counts.into_iter().max_by_key(|(_, n)| *n).map(|(class, _)| class)
    }
}

#[cfg(test)]
mod tests;
