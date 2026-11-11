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

//! VerseKit as a "Log Watcher" for scmdb.net (SCMDB Log Watcher Protocol 0.1.9,
//! <https://scmdb.net/docs/watcher-protocol.md>), port of `scmdb_bridge.py`.
//!
//! A small web server **only on the own computer** (`127.0.0.1:23456`). The scmdb page in the player's browser
//! connects to it once its switch "Watcher" is on, takes received blueprints into the scmdb account and starts
//! timers for running contracts. VerseKit itself sends nothing into the network.
//!
//! | Address | Content |
//! |---|---|
//! | `GET /ping` | protocol version, channel, `client`, `clientVersion` |
//! | `GET /events` | Server-Sent Events: first the history of the running session (`replay: true`), then the state now, then new events |
//!
//! Where the events come from (all from the running `Game.log`):
//!
//! | Event | Log line |
//! |---|---|
//! | `mission_start` | the first `CreateMarker` line of a `missionId`; it carries contract, generator and `contractDefinitionId` |
//! | `mission_complete` / `mission_ended` | `<EndMission> … CompletionType[…] Reason[…]` |
//! | `mission_ended` (`Disconnect`) | leaving the game world (`RequestFrontEnd`); the game reports no end then |
//! | `blueprint_received` | the blueprint message (`phrases::pattern`) |
//! | `session_reset` | the `Game.log` begins anew |
//!
//! Rules: off by default (setting [`SETTING`]); the **English catalog name** is sent, not the name of the log (a
//! German name or one with class and size added cannot be matched by scmdb for sure), a name without one catalog
//! entry is not sent and only noted in the trail; only the running session, no old `logbackups/`; the channel
//! honestly on every event (scmdb takes over only LIVE and HOTFIX); a port in use (the official watcher, say):
//! no start, only the state says so; CORS only for `https://scmdb.net` and `https://www.scmdb.net`, never `*`.
//!
//! ⚠ The SSE stream is the one 4.12 writes byte for byte: the events as `json.dumps(…, ensure_ascii=False)`
//! writes them (keys in the order of 4.12, `, ` and `: ` between, floats as Python's `repr`).

use std::collections::{HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use regex::Regex;

use crate::jsonio::{self, Map, Value};
use crate::paths::{CHANNELS, Host, Paths};
use crate::phrases::Sources;
use crate::pyfloat;

/// The switch in the settings; off by default.
pub const SETTING: &str = "scmdb_bruecke";
pub const HOST: &str = "127.0.0.1";
pub const PORT: u16 = 23456;
pub const PROTOCOL: &str = "0.1.9";
pub const CLIENT: &str = "versekit";
pub const ORIGINS: [&str; 2] = ["https://scmdb.net", "https://www.scmdb.net"];
/// So many events of the session are replayed to a new listener.
pub const HISTORY: usize = 500;
pub const HEARTBEAT: Duration = Duration::from_secs(15);
pub const POLL: Duration = Duration::from_secs(1);
/// A blueprint belongs to the contract accepted or completed at most so many seconds before.
pub const MISSION_WINDOW: f64 = 120.0;
/// Another port (test runs); the old name [`crate::legacy::ENV_SCMDB_PORT`] still counts.
pub const ENV_PORT: &str = "VERSEKIT_SCMDB_PORT";

static TIMESTAMP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(\.\d+)?Z>").expect("pattern"));
static MARKER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"CreateMarker>[^\n]*?missionId \[([0-9a-fA-F-]+)\][^\n]*?",
        r"generator name \[([^\]]*)\][^\n]*?contract \[([^\]]*)\][^\n]*?",
        r"contractDefinitionId\[([0-9a-fA-F-]+)\]"
    ))
    .expect("pattern")
});
static END: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"<EndMission>[^\n]*?MissionId\[([0-9a-fA-F-]+)\][^\n]*?",
        r"CompletionType\[([^\]]*)\](?:[^\n]*?Reason\[([^\]]*)\])?"
    ))
    .expect("pattern")
});
static LEFT_GAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"CSessionManager::RequestFrontEnd\]\s*Started").expect("pattern"));

/// Whether the switch is on.
pub fn enabled(paths: &Paths) -> bool {
    paths.setting_bool(SETTING, false)
}

/// The port: [`ENV_PORT`], else the old name, else [`PORT`]; a value that is no port counts as none.
pub fn port(host: &Host) -> u16 {
    let given = host.var_set(ENV_PORT).or_else(|| host.var_set(crate::legacy::ENV_SCMDB_PORT));
    given.and_then(|v| v.trim().parse::<u16>().ok()).unwrap_or(PORT)
}

/// `channel_of`: LIVE, HOTFIX, PTU, EPTU, TECH-PREVIEW from the name of the folder, else UNKNOWN.
pub fn channel_of(folder: Option<&str>) -> String {
    let name = folder.unwrap_or_default().trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or_default();
    let name = name.to_uppercase();
    if CHANNELS.contains(&name.as_str()) { name } else { "UNKNOWN".to_string() }
}

/// Days since 1970-01-01 of a date of the Gregorian calendar.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// `line_time`: the moment of a log line in Unix seconds to three places, or now.
pub fn line_time(line: &str) -> f64 {
    let Some(found) = TIMESTAMP.captures(line) else { return pyfloat::round(now(), 3) };
    let part = |i: usize| found[i].parse::<i64>().unwrap_or(0);
    let stamp = days_from_civil(part(1), part(2), part(3)) * 86_400 + part(4) * 3600 + part(5) * 60 + part(6);
    let fraction = found.get(7).and_then(|m| m.as_str().parse::<f64>().ok()).unwrap_or(0.0);
    pyfloat::round(stamp as f64 + fraction, 3)
}

/// What is known of a running contract.
#[derive(Clone, Debug, Default, PartialEq)]
struct Info {
    debug_name: Option<String>,
    generator: Option<String>,
    definition: Option<String>,
    start: Option<f64>,
}

fn text_or_null(text: &Option<String>) -> Value {
    text.as_ref().map_or(Value::Null, |t| Value::str(t.clone()))
}

fn float(x: f64) -> Value {
    Value::Float(x)
}

/// The English catalog name of a blueprint name of the log, or `None` when it has no one catalog entry.
pub type EnglishName<'a> = Box<dyn Fn(&str) -> Option<String> + Send + 'a>;

/// `Reader`: makes protocol events of log lines. Without network, without thread.
pub struct Reader<'a> {
    pub channel: String,
    active: Vec<(String, Info)>,
    ended: HashSet<String>,
    last_mission: Option<(String, Info, &'static str, f64)>,
    english: EnglishName<'a>,
    pattern: Regex,
    sources: Sources,
    /// The names of blueprints that were not sent since the last look.
    pub unmatched: Vec<String>,
}

impl<'a> Reader<'a> {
    pub fn new(channel: &str, english: EnglishName<'a>, pattern: Regex, sources: Sources) -> Reader<'a> {
        Reader {
            channel: channel.to_string(),
            active: Vec::new(),
            ended: HashSet::new(),
            last_mission: None,
            english,
            pattern,
            sources,
            unmatched: Vec::new(),
        }
    }

    /// The same reader on another channel and with nothing read yet (a new session).
    fn fresh(&mut self, channel: &str) {
        self.channel = channel.to_string();
        self.active.clear();
        self.ended.clear();
        self.last_mission = None;
        self.unmatched.clear();
    }

    fn event(&self, kind: &str, fields: Map) -> Value {
        let mut event =
            vec![("type".to_string(), Value::str(kind)), ("channel".to_string(), Value::str(self.channel.clone()))];
        event.extend(fields);
        Value::Object(event)
    }

    fn mission_fields(guid: &str, info: Option<&Info>) -> Map {
        let empty = Info::default();
        let info = info.unwrap_or(&empty);
        vec![
            ("guid".to_string(), Value::str(guid)),
            ("debugName".to_string(), text_or_null(&info.debug_name)),
            ("generator".to_string(), text_or_null(&info.generator)),
            ("contractDefinitionId".to_string(), text_or_null(&info.definition)),
        ]
    }

    /// `feed`: one log line, its events (mostly none).
    pub fn feed(&mut self, line: &str) -> Vec<Value> {
        let mut out = Vec::new();
        if line.contains("CreateMarker>") {
            if let Some(m) = MARKER.captures(line) {
                let guid = m[1].to_string();
                if !self.active.iter().any(|(g, _)| *g == guid) && !self.ended.contains(&guid) {
                    let some = |i: usize| Some(m[i].to_string()).filter(|s| !s.is_empty());
                    let info = Info {
                        debug_name: some(3),
                        generator: some(2),
                        definition: some(4),
                        start: Some(line_time(line)),
                    };
                    let start = info.start.unwrap_or_default();
                    self.active.push((guid.clone(), info.clone()));
                    self.last_mission = Some((guid.clone(), info.clone(), "accept", start));
                    let mut fields = Reader::mission_fields(&guid, Some(&info));
                    fields.push(("startTs".to_string(), float(start)));
                    out.push(self.event("mission_start", fields));
                }
            }
        } else if line.contains("<EndMission>") {
            let Some(m) = END.captures(line) else { return out };
            let guid = m[1].to_string();
            let running = self.active.iter().position(|(g, _)| *g == guid);
            if self.ended.contains(&guid) && running.is_none() {
                // Ended already (on leaving the game world, say): no second end.
                return out;
            }
            let completion = Some(m[2].to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Unknown".to_string());
            let info = running.map(|i| self.active.remove(i).1);
            self.ended.insert(guid.clone());
            let kind = if completion == "Complete" { "mission_complete" } else { "mission_ended" };
            let end = line_time(line);
            let mut fields = Reader::mission_fields(&guid, info.as_ref());
            let reason = m.get(3).map_or("", |r| r.as_str());
            fields.push(("completion".to_string(), Value::str(completion.clone())));
            fields.push(("reason".to_string(), Value::str(reason)));
            fields.push(("endTs".to_string(), float(end)));
            if completion == "Complete" {
                self.last_mission = Some((guid, info.unwrap_or_default(), "complete", end));
            }
            out.push(self.event(kind, fields));
        } else if LEFT_GAME.is_match(line) {
            let when = line_time(line);
            for (guid, info) in std::mem::take(&mut self.active) {
                let mut fields = Reader::mission_fields(&guid, Some(&info));
                fields.push(("completion".to_string(), Value::str("Disconnect")));
                fields.push(("reason".to_string(), Value::str("Left game")));
                fields.push(("endTs".to_string(), float(when)));
                out.push(self.event("mission_ended", fields));
                self.ended.insert(guid);
            }
        } else if line.contains("Added notification") {
            for (name, _extra) in crate::logscan::names_from_text(line, &self.pattern, &self.sources) {
                let Some(english) = (self.english)(&name) else {
                    self.unmatched.push(name);
                    continue;
                };
                let when = line_time(line);
                let mut mission = (Value::Null, Value::Null, Value::Null, Value::Null);
                if let Some((guid, info, trigger, at)) = &self.last_mission
                    && when - at <= MISSION_WINDOW
                {
                    mission = (
                        Value::str(guid.clone()),
                        text_or_null(&info.debug_name),
                        text_or_null(&info.definition),
                        Value::str(*trigger),
                    );
                }
                let fields = vec![
                    ("productName".to_string(), Value::str(english)),
                    ("missionGuid".to_string(), mission.0),
                    ("missionDebugName".to_string(), mission.1),
                    ("missionContractDefinitionId".to_string(), mission.2),
                    ("missionTrigger".to_string(), mission.3),
                    ("ts".to_string(), float(when)),
                ];
                out.push(self.event("blueprint_received", fields));
            }
        }
        out
    }

    /// `snapshot`: the contracts running now.
    pub fn snapshot(&self) -> Value {
        let active = self
            .active
            .iter()
            .map(|(guid, info)| {
                let mut fields = Reader::mission_fields(guid, Some(info));
                fields.push(("startTs".to_string(), info.start.map_or(Value::Null, float)));
                Value::Object(fields)
            })
            .collect();
        self.event("state_snapshot", vec![("active".to_string(), Value::Array(active))])
    }
}

/// `english_name`: the English catalog name of `name` (the name the catalog gives, else the one found), or
/// `None` when the catalog is empty or knows no entry for it.
pub fn english_name(
    collection: &crate::collection::Collection,
    known: &crate::collection::Known,
    name: &str,
) -> Option<String> {
    if known.is_empty() || name.is_empty() {
        return None;
    }
    let found = collection.catalog_name(name, Some(known));
    match known.get(&crate::paths::name_key(&found))? {
        Some(english) => Some(english.to_string()),
        None => Some(found),
    }
}

/// The namer of the bridge on the blueprints `known`: the catalog name as the collection finds it (also through the
/// reverse lookup of the installation's texts), then its English name.
pub fn namer(paths: &Paths, known: crate::collection::Known) -> EnglishName<'static> {
    let paths = paths.working_copy();
    Box::new(move |name| {
        let names = crate::phrases::BackRoute { paths: &paths, archive: &crate::phrases::NoArchive };
        let collection = crate::collection::Collection {
            paths: &paths,
            catalog: &crate::collection::NoCatalog,
            names: &names,
            exports: &crate::collection::NoExports,
            clock: crate::collection::now_text,
        };
        english_name(&collection, &known, name)
    })
}

/// One event as one message of the stream: `data: <JSON>` and an empty line.
pub fn message(event: &Value) -> String {
    format!("data: {}\n\n", jsonio::dumps(event, None, false))
}

/// `dict(event, replay=True)`.
fn replayed(event: &Value) -> Value {
    match event {
        Value::Object(map) => {
            let mut map = map.clone();
            jsonio::map_set(&mut map, "replay", Value::Bool(true));
            Value::Object(map)
        }
        other => other.clone(),
    }
}

// ------------------------------------------------------------ the server

/// What the bridge is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Off,
    /// The server runs, no scmdb page is connected.
    Waiting,
    Connected,
    /// The port is in use (by the official watcher, say): not started.
    Busy,
    /// The port could not be taken for another reason than "in use": not started. Under Windows that can be a range
    /// of ports the system keeps for itself, a rule, or another program holding it exclusively (with
    /// `SO_EXCLUSIVEADDRUSE` the answer may be 10013 instead of 10048), the official watcher among them. Which one,
    /// only the reason in the status says.
    Refused,
}

/// `STATUS`: what the switch reports.
#[derive(Clone, Debug, PartialEq)]
pub struct Status {
    pub state: State,
    pub clients: usize,
    pub channel: String,
    pub sent: usize,
    pub unmatched: usize,
    /// The port asked for at the last start.
    pub port: u16,
    /// Why the last start did not listen, in the words of the system (with its number).
    pub error: Option<String>,
}

impl Default for Status {
    fn default() -> Status {
        Status {
            state: State::Off,
            clients: 0,
            channel: "UNKNOWN".into(),
            sent: 0,
            unmatched: 0,
            port: PORT,
            error: None,
        }
    }
}

/// The state of a start whose port could not be taken: in use, or refused for another reason.
fn failed_state(error: &std::io::Error) -> State {
    if error.kind() == std::io::ErrorKind::AddrInUse { State::Busy } else { State::Refused }
}

/// The line of the start trail about the bridge (`on`: the switch), so that a report shows whether it listens and,
/// where not, why.
pub fn trail_line(on: bool, status: &Status) -> String {
    let port = status.port;
    let error = status.error.as_deref().unwrap_or("—");
    match (on, status.state) {
        (false, _) => "scmdb-Brücke: aus".to_string(),
        (true, State::Waiting | State::Connected) => format!("scmdb-Brücke: lauscht auf {HOST}:{port}"),
        (true, State::Busy) => format!("scmdb-Brücke: Anschluss {port} belegt ({error})"),
        (true, State::Refused) => format!("scmdb-Brücke: Anschluss {port} nicht nutzbar ({error})"),
        (true, State::Off) => {
            "scmdb-Brücke: an, aber nicht gestartet (ohne eigene Daten oder im Selbsttest)".to_string()
        }
    }
}

/// A listener of the stream: its number and where the next event goes (`None` when the bridge stops).
type Listener = (u64, Sender<Option<Value>>);

struct Shared {
    history: VecDeque<Value>,
    listeners: Vec<Listener>,
    next_listener: u64,
    /// The channel of the session read, for `/ping`.
    channel: String,
    /// The contracts running now (`reader.snapshot()`), for a listener who connects.
    snapshot: Option<Value>,
    status: Status,
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the bridge needs from the program: the folders, the version and how names become English catalog names.
pub struct Setup {
    pub host: Host,
    /// The version of the program for `clientVersion` (at most 16 characters are sent).
    pub version: String,
    /// Makes the namer of the reader, in the thread that reads the log.
    pub namer: Box<dyn Fn(&Paths) -> EnglishName<'static> + Send + Sync>,
    pub poll: Duration,
}

/// `Bridge`: server and reading along. [`Bridge::start`] and [`Bridge::stop`] may come several times.
pub struct Bridge {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    running: bool,
}

impl Default for Bridge {
    fn default() -> Bridge {
        Bridge::new()
    }
}

impl Bridge {
    pub fn new() -> Bridge {
        let shared = Shared {
            history: VecDeque::new(),
            listeners: Vec::new(),
            next_listener: 0,
            channel: "UNKNOWN".into(),
            snapshot: None,
            status: Status::default(),
        };
        Bridge { shared: Arc::new(Mutex::new(shared)), stop: Arc::new(AtomicBool::new(false)), running: false }
    }

    pub fn status(&self) -> Status {
        lock(&self.shared).status.clone()
    }

    /// `start`: the server on `127.0.0.1` and the reading of the log. `false` when the port cannot be taken; the
    /// status says whether it is in use ([`State::Busy`]) or refused ([`State::Refused`]), and why.
    pub fn start(&mut self, setup: Setup) -> bool {
        self.start_with(setup, listen)
    }

    /// [`Bridge::start`] with the port taken by `open` (the checks make up a refusal of the system).
    fn start_with(&mut self, setup: Setup, open: impl FnOnce(u16) -> std::io::Result<TcpListener>) -> bool {
        if self.running {
            return true;
        }
        let port = port(&setup.host);
        let listener = match open(port) {
            Ok(listener) => listener,
            Err(error) => {
                let mut shared = lock(&self.shared);
                shared.status.state = failed_state(&error);
                shared.status.clients = 0;
                shared.status.port = port;
                shared.status.error = Some(error.to_string());
                return false;
            }
        };
        self.stop = Arc::new(AtomicBool::new(false));
        self.running = true;
        {
            // The log is read from its start again: the history of a run before is dropped, so no event comes twice.
            let mut shared = lock(&self.shared);
            shared.history.clear();
            shared.snapshot = None;
            shared.status.state = State::Waiting;
            shared.status.clients = 0;
            shared.status.port = port;
            shared.status.error = None;
        }
        let version: String = setup.version.chars().take(16).collect();
        let (shared, stop) = (Arc::clone(&self.shared), Arc::clone(&self.stop));
        let _ = std::thread::Builder::new()
            .name("scmdb-server".into())
            .spawn(move || serve(listener, shared, stop, version));
        let (shared, stop) = (Arc::clone(&self.shared), Arc::clone(&self.stop));
        let _ = std::thread::Builder::new().name("scmdb-log".into()).spawn(move || tail(setup, shared, stop));
        true
    }

    /// `stop`: server and reading end, every listener is let go.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.running = false;
        let mut shared = lock(&self.shared);
        for (_, listener) in shared.listeners.drain(..) {
            let _ = listener.send(None);
        }
        shared.status.state = State::Off;
        shared.status.clients = 0;
        // A reason of a start before is no reason now: the trail of a later start without own data says only "off".
        shared.status.error = None;
    }

    /// `apply_setting`: switched on, it starts; switched off, it stops.
    pub fn apply(&mut self, on: bool, setup: impl FnOnce() -> Setup) -> bool {
        if on {
            self.start(setup())
        } else {
            self.stop();
            true
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `publish`: events into the history and to every listener.
fn publish(shared: &Mutex<Shared>, events: Vec<Value>) {
    if events.is_empty() {
        return;
    }
    let mut shared = lock(shared);
    let listeners = shared.listeners.clone();
    for event in events {
        if shared.history.len() == HISTORY {
            shared.history.pop_front();
        }
        shared.history.push_back(event.clone());
        for (_, listener) in &listeners {
            let _ = listener.send(Some(event.clone()));
        }
        if !listeners.is_empty() {
            shared.status.sent += 1;
        }
    }
}

/// `reset`: a new session of the game; the history is emptied first, then it is said.
fn reset(shared: &Mutex<Shared>, channel: &str) {
    let mut shared = lock(shared);
    shared.history.clear();
    shared.channel = channel.to_string();
    let event =
        Value::Object(vec![("type".into(), Value::str("session_reset")), ("channel".into(), Value::str(channel))]);
    for (_, listener) in &shared.listeners {
        let _ = listener.send(Some(event.clone()));
    }
}

/// `_tail`: the running `Game.log` read along, from its start; a log that begins anew (another file, shorter, or
/// another head) is a new session.
fn tail(setup: Setup, shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>) {
    let paths = Paths::new(setup.host.clone());
    let sources = crate::logsource::LogSource::sources_of(&paths);
    let pattern = crate::phrases::pattern(&sources.collect().0);
    let mut reader = Reader::new("UNKNOWN", (setup.namer)(&paths), pattern, sources);
    let mut file: Option<String> = None;
    let (mut position, mut head, mut rest) = (0u64, Vec::new(), Vec::new());
    while !stop.load(Ordering::SeqCst) {
        let folder = paths.game_folder();
        if let Some(current) = paths.game_log(folder.as_deref()) {
            let channel = channel_of(folder.as_deref());
            if let Ok(mut handle) = std::fs::File::open(&current) {
                let size = handle.metadata().map_or(0, |m| m.len());
                let mut first = vec![0u8; 256];
                let got = handle.read(&mut first).unwrap_or(0);
                first.truncate(got);
                let rotated = file.as_deref() != Some(current.as_str())
                    || size < position
                    || first.get(..head.len()) != Some(head.as_slice());
                if rotated {
                    if file.is_some() {
                        reset(&shared, &channel);
                    }
                    reader.fresh(&channel);
                    (file, position, head, rest) = (Some(current.clone()), 0, first, Vec::new());
                    let mut guard = lock(&shared);
                    guard.channel = channel.clone();
                    guard.status.channel = channel.clone();
                    guard.snapshot = Some(reader.snapshot());
                }
                let mut chunk = Vec::new();
                if handle.seek(SeekFrom::Start(position)).is_ok() {
                    let _ = handle.read_to_end(&mut chunk);
                }
                position += chunk.len() as u64;
                let mut data = std::mem::take(&mut rest);
                data.extend_from_slice(&chunk);
                match data.iter().rposition(|b| *b == b'\n') {
                    Some(cut) => {
                        rest = data[cut + 1..].to_vec();
                        let text = String::from_utf8_lossy(&data[..cut]).into_owned();
                        let mut events = Vec::new();
                        for line in text.split('\n') {
                            events.extend(reader.feed(line));
                        }
                        publish(&shared, events);
                        if !reader.unmatched.is_empty() {
                            for name in &reader.unmatched {
                                crate::trail::step(&format!(
                                    "scmdb: Bauplan nicht zugeordnet, nicht geschickt: {name}"
                                ));
                            }
                            lock(&shared).status.unmatched += reader.unmatched.len();
                            reader.unmatched.clear();
                        }
                        // The state now, for a listener who comes later (`subscribe`).
                        lock(&shared).snapshot = Some(reader.snapshot());
                    }
                    None => rest = data,
                }
            }
        }
        sleep_unless(&stop, setup.poll);
    }
}

fn sleep_unless(stop: &AtomicBool, wait: Duration) {
    let step = Duration::from_millis(50);
    let mut left = wait;
    while !stop.load(Ordering::SeqCst) && !left.is_zero() {
        let now = left.min(step);
        std::thread::sleep(now);
        left -= now;
    }
}

/// The port taken alone. ⚠ Under Windows `SO_REUSEADDR` lets a second program sit on the same port, and
/// VerseKit and the watcher of scmdb would run side by side in silence: there `SO_EXCLUSIVEADDRUSE`.
fn listen(port: u16) -> std::io::Result<TcpListener> {
    #[cfg(windows)]
    {
        exclusive::listen(port)
    }
    #[cfg(not(windows))]
    {
        TcpListener::bind((HOST, port))
    }
}

#[cfg(windows)]
mod exclusive {
    use std::net::TcpListener;
    use std::os::windows::io::FromRawSocket;

    use windows_sys::Win32::Networking::WinSock::{
        AF_INET, IN_ADDR, IN_ADDR_0, INVALID_SOCKET, IPPROTO_TCP, SO_EXCLUSIVEADDRUSE, SOCK_STREAM, SOCKADDR,
        SOCKADDR_IN, SOL_SOCKET, SOMAXCONN, WSADATA, WSAStartup, bind, closesocket, listen as listen_on, setsockopt,
        socket,
    };

    pub fn listen(port: u16) -> std::io::Result<TcpListener> {
        // SAFETY: the calls get values that live during them; the socket is closed on every way out but the one
        // that hands it to the listener, which owns it then.
        unsafe {
            let mut data: WSADATA = std::mem::zeroed();
            WSAStartup(0x0202, &mut data);
            let raw = socket(i32::from(AF_INET), SOCK_STREAM, IPPROTO_TCP);
            if raw == INVALID_SOCKET {
                return Err(std::io::Error::last_os_error());
            }
            let on: i32 = 1;
            let fail = |raw| {
                let error = std::io::Error::last_os_error();
                closesocket(raw);
                Err(error)
            };
            if setsockopt(raw, SOL_SOCKET, SO_EXCLUSIVEADDRUSE, (&raw const on).cast(), 4) != 0 {
                return fail(raw);
            }
            let address = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: port.to_be(),
                sin_addr: IN_ADDR { S_un: IN_ADDR_0 { S_addr: u32::from_ne_bytes([127, 0, 0, 1]) } },
                sin_zero: [0; 8],
            };
            if bind(raw, (&raw const address).cast::<SOCKADDR>(), std::mem::size_of::<SOCKADDR_IN>() as i32) != 0 {
                return fail(raw);
            }
            if listen_on(raw, SOMAXCONN as i32) != 0 {
                return fail(raw);
            }
            Ok(TcpListener::from_raw_socket(raw as u64))
        }
    }
}

/// Who answers `/ping` on the port of `host` on this computer: the `client` of the answer (empty when it names
/// none, as the official watcher of scmdb), or `None` when nobody answers within `wait`.
pub fn who_answers(host: &Host, wait: Duration) -> Option<String> {
    let port = port(host);
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, wait).ok()?;
    stream.set_read_timeout(Some(wait)).ok()?;
    let request = format!("GET /ping HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let text = String::from_utf8_lossy(&answer);
    let (head, body) = text.split_once("\r\n\r\n")?;
    head.starts_with("HTTP/").then_some(())?;
    let client =
        jsonio::loads(body.trim()).ok().and_then(|v| v.get("client").and_then(Value::as_str).map(str::to_string));
    Some(client.unwrap_or_default())
}

/// `host_allowed`: `127.0.0.1:<port>`, `localhost:<port>` or `[::1]:<port>`, and those names without a port;
/// nothing without a `Host` head.
pub fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host.filter(|h| !h.is_empty()) else { return false };
    let host = crate::text::py_strip(host).to_lowercase();
    // `rpartition(':')`: a port only when what follows the last colon is digits.
    let (name, given) = match host.rsplit_once(':') {
        Some((name, given)) if !given.is_empty() && given.chars().all(|c| c.is_ascii_digit()) => (name, given),
        _ => (host.as_str(), ""),
    };
    if !given.is_empty() && given != port.to_string() {
        return false;
    }
    ["127.0.0.1", "localhost", "[::1]"].contains(&name)
}

/// A request of the browser: method, path and heads.
struct Request {
    method: String,
    path: String,
    heads: Vec<(String, String)>,
}

impl Request {
    fn head(&self, name: &str) -> Option<&str> {
        self.heads.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// Reads one request; `None` when the browser closed the connection or sent something else.
fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut heads = Vec::new();
    loop {
        let mut head = String::new();
        if reader.read_line(&mut head).ok()? == 0 {
            return None;
        }
        let head = head.trim_end_matches(['\r', '\n']);
        if head.is_empty() {
            break;
        }
        if let Some((name, value)) = head.split_once(':') {
            heads.push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    Some(Request { method, path, heads })
}

/// The date of an HTTP head (`Thu, 30 Oct 2026 07:40:00 GMT`).
fn http_date() -> String {
    let seconds = now() as i64;
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (h, m, s) = (rest / 3600, rest % 3600 / 60, rest % 60);
    // Civil date from the days (inverse of `days_from_civil`).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let of_era = z - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * of_year + 2) / 153;
    let day = of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize];
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{weekday}, {day:02} {} {year} {h:02}:{m:02}:{s:02} GMT", months[(month - 1) as usize])
}

/// `_head`: the status line and the heads; for a page of scmdb also the heads of CORS and of Private Network
/// Access (Chrome asks for them before a public page reaches the own computer).
fn head(request: &Request, status: &str, heads: &[(&str, String)]) -> String {
    let mut out = format!("HTTP/1.1 {status}\r\nServer: VerseKit\r\nDate: {}\r\n", http_date());
    if let Some(origin) = request.head("Origin").filter(|o| ORIGINS.contains(o)) {
        out.push_str(&format!(
            "Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\nAccess-Control-Allow-Private-Network: true\r\n"
        ));
    }
    for (name, value) in heads {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str("\r\n");
    out
}

fn serve(listener: TcpListener, shared: Arc<Mutex<Shared>>, stop: Arc<AtomicBool>, version: String) {
    let port = listener.local_addr().map_or(PORT, |a| a.port());
    let _ = listener.set_nonblocking(true);
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let (shared, stop, version) = (Arc::clone(&shared), Arc::clone(&stop), version.clone());
                let _ = std::thread::Builder::new()
                    .name("scmdb-verbindung".into())
                    .spawn(move || connection(stream, port, &shared, &stop, &version));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// One connection of the browser: requests one after the other until it closes or asks for the stream.
fn connection(stream: TcpStream, port: u16, shared: &Arc<Mutex<Shared>>, stop: &AtomicBool, version: &str) {
    let Ok(writer) = stream.try_clone() else { return };
    let mut writer = writer;
    let mut reader = BufReader::new(stream);
    while let Some(request) = read_request(&mut reader) {
        // ⚠ Against DNS rebinding: a foreign page can point its name at 127.0.0.1 and then counts as the same
        // origin for the browser, CORS does not hold. The `Host` head still carries its name.
        let close = request.head("Connection").is_some_and(|c| c.eq_ignore_ascii_case("close"));
        if !host_allowed(request.head("Host"), port) {
            let _ = writer.write_all(head(&request, "403 Forbidden", &[("Content-Length", "0".into())]).as_bytes());
            if close {
                break;
            }
            continue;
        }
        let route = request.path.split('?').next().unwrap_or_default();
        match (request.method.as_str(), route) {
            ("OPTIONS", _) => {
                let heads = [
                    ("Access-Control-Allow-Methods", "GET, OPTIONS".to_string()),
                    ("Access-Control-Allow-Headers", "*".to_string()),
                    ("Content-Length", "0".to_string()),
                ];
                let _ = writer.write_all(head(&request, "204 No Content", &heads).as_bytes());
            }
            ("GET", "/ping") => {
                let channel = lock(shared).channel.clone();
                let body = jsonio::dumps(
                    &Value::Object(vec![
                        ("status".into(), Value::str("ok")),
                        ("version".into(), Value::str(PROTOCOL)),
                        ("channel".into(), Value::str(channel)),
                        ("client".into(), Value::str(CLIENT)),
                        ("clientVersion".into(), Value::str(version)),
                    ]),
                    None,
                    false,
                );
                let heads = [
                    ("Content-Type", "application/json".to_string()),
                    ("Cache-Control", "no-cache".to_string()),
                    ("Content-Length", body.len().to_string()),
                ];
                let _ = writer.write_all(format!("{}{body}", head(&request, "200 OK", &heads)).as_bytes());
            }
            ("GET", "/events") => {
                events(&request, writer, shared, stop);
                return;
            }
            ("GET", _) => {
                let _ = writer.write_all(head(&request, "404 Not Found", &[("Content-Length", "0".into())]).as_bytes());
            }
            _ => {
                let body = "Unsupported method";
                let heads = [("Content-Type", "text/plain".to_string()), ("Content-Length", body.len().to_string())];
                let _ = writer.write_all(format!("{}{body}", head(&request, "501 Not Implemented", &heads)).as_bytes());
            }
        }
        let _ = writer.flush();
        // A browser that asks for the end of the connection gets it after the answer.
        if close {
            break;
        }
    }
}

/// `subscribe`: a new listener with the history as replay and the state now.
fn subscribe(shared: &Mutex<Shared>) -> (u64, Receiver<Option<Value>>, Vec<Value>, Value) {
    let (tx, rx) = mpsc::channel();
    let mut guard = lock(shared);
    let replay: Vec<Value> = guard.history.iter().map(replayed).collect();
    let channel = guard.channel.clone();
    let snapshot = guard.snapshot.clone().unwrap_or_else(|| {
        Value::Object(vec![
            ("type".into(), Value::str("state_snapshot")),
            ("channel".into(), Value::str(channel)),
            ("active".into(), Value::Array(Vec::new())),
        ])
    });
    let number = guard.next_listener;
    guard.next_listener += 1;
    guard.listeners.push((number, tx));
    guard.status.clients = guard.listeners.len();
    guard.status.state = State::Connected;
    (number, rx, replay, snapshot)
}

/// `unsubscribe`: the listener `number` is gone.
fn unsubscribe(shared: &Mutex<Shared>, number: u64) {
    let mut guard = lock(shared);
    guard.listeners.retain(|(n, _)| *n != number);
    guard.status.clients = guard.listeners.len();
    if guard.status.state != State::Off {
        guard.status.state = if guard.listeners.is_empty() { State::Waiting } else { State::Connected };
    }
}

/// `_events`: the stream; history, state now, then live with a heartbeat every [`HEARTBEAT`].
fn events(request: &Request, mut writer: TcpStream, shared: &Arc<Mutex<Shared>>, stop: &AtomicBool) {
    let heads = [
        ("Content-Type", "text/event-stream".to_string()),
        ("Cache-Control", "no-cache".to_string()),
        ("Connection", "close".to_string()),
    ];
    if writer.write_all(head(request, "200 OK", &heads).as_bytes()).is_err() {
        return;
    }
    let (number, rx, replay, snapshot) = subscribe(shared);
    let mut send = |text: &str| writer.write_all(text.as_bytes()).and_then(|()| writer.flush()).is_ok();
    let mut open = replay.iter().all(|event| send(&message(event))) && send(&message(&snapshot));
    while open && !stop.load(Ordering::SeqCst) {
        match rx.recv_timeout(HEARTBEAT) {
            Ok(Some(event)) => open = send(&message(&event)),
            Ok(None) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => open = send(": heartbeat\n\n"),
        }
    }
    unsubscribe(shared, number);
}

#[cfg(test)]
mod tests;
