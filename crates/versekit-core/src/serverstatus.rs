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

//! The state of the servers of CIG (`serverstatus.py` of 4.12): the status page
//! `status.robertsspaceindustries.com` (a static page: `index.json` for the
//! systems, `issues/index.json` for the history, `issues/<name>/index.json` for
//! one incident in full). What it said is kept in `serverstatus.json` in the
//! form of 4.12 (`etag`, `lage` with `gesamt`, `systeme`, `geholt`, `stand`,
//! `quelle`; `volltexte` for the full texts of resolved incidents), so that both
//! versions read what the other fetched. With `SC_BP_NO_NET` nothing is asked;
//! the state kept stands.
//!
//! The page is asked with the tag of the last answer (`If-None-Match`): when
//! nothing changed it answers `304` without a body, so asking every minute
//! costs almost nothing. A forced look asks without the tag.
//!
//! ⚠ The words of CIG (`operational`, `maintenance` …, titles, texts) are shown
//! as they are, never translated, as in 4.12. They are a notice of CIG, not a
//! measurement: the time asked and the source belong beside them.
//!
//! ⚠ The page counts in UTC, also where no zone is written.

use std::sync::LazyLock;

use regex::Regex;

use crate::jsonio::{self, Map, Value};
use crate::net::{Fresh, Net};
use crate::paths::Paths;

/// The status page.
pub const BASE: &str = "https://status.robertsspaceindustries.com";
/// The file of the state kept (in the folder `Intern`).
pub const CACHE: &str = "serverstatus.json";
/// How old the state kept may be before the page is asked again.
pub const FRESH_SECONDS: f64 = 300.0;
/// How often the page is asked while it is open (seconds).
pub const POLL_SECONDS: f64 = 60.0;
/// How many incidents of the history are looked at for the latest notices.
const HISTORY_LOOKED_AT: usize = 60;

/// The light of each condition (`LIGHTS`); anything unknown counts as a disruption.
pub fn light(condition: &str) -> &'static str {
    match condition {
        "operational" => "ok",
        "monitoring" | "maintenance" => "hinweis",
        "major_outage" => "aus",
        _ => "gestoert",
    }
}

/// The colours of the lights when the page names none (`COLORS`).
fn default_colour(light: &str) -> &'static str {
    match light {
        "ok" => "#008000",
        "aus" => "#e60000",
        "hinweis" => "#24478f",
        _ => "#cc4400",
    }
}

/// How bad a light is: the worst one gives the colour of the whole.
fn rank(light: &str) -> u8 {
    match light {
        "ok" => 0,
        "hinweis" => 1,
        "aus" => 3,
        _ => 2,
    }
}

/// One incident: as it comes with a system or in the history, and with its text when fetched in full.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Incident {
    pub title: String,
    /// In the word of CIG (`maintenance`, `disrupted` …).
    pub severity: String,
    pub affected: Vec<String>,
    /// Seconds since 1970.
    pub begun: Option<i64>,
    pub resolved: Option<i64>,
    /// The file of the incident (`2026-08-26_live-deployment.md`).
    pub file: String,
    pub address: String,
    /// The lines of the text, each with whether it is highlighted throughout.
    pub lines: Vec<(String, bool)>,
}

/// One system of the page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct System {
    pub name: String,
    /// The condition in the word of CIG.
    pub status: String,
    /// `ok`, `hinweis`, `gestoert` or `aus`.
    pub light: String,
    /// The colour the page gives that light (`#rrggbb`).
    pub colour: String,
    /// The incidents not yet resolved.
    pub incidents: Vec<Incident>,
}

/// What the page said.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// The overall word of CIG (`operational` …).
    pub overall: String,
    pub systems: Vec<System>,
    /// When the page was asked (seconds).
    pub fetched: f64,
    /// When CIG changed the page last (seconds).
    pub stand: Option<i64>,
    pub source: String,
    /// The page could not be asked; what stands is the state kept, if any.
    pub no_net: bool,
}

impl State {
    /// Nothing was ever fetched.
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    /// No disruption is reported.
    pub fn all_well(&self) -> bool {
        self.overall.to_lowercase() == "operational"
    }

    /// The colour of the whole: the one of the worst system.
    pub fn colour(&self) -> Option<&str> {
        let mut worst: Option<&System> = None;
        for system in &self.systems {
            if worst.is_none_or(|w| rank(&system.light) > rank(&w.light)) {
                worst = Some(system);
            }
        }
        worst.map(|system| system.colour.as_str()).filter(|c| !c.is_empty())
    }

    /// What shows whether something changed: without the time asked.
    fn core(&self) -> Option<Core> {
        if self.is_empty() && self.overall.is_empty() {
            return None;
        }
        let systems = self
            .systems
            .iter()
            .map(|system| {
                let mut titles: Vec<String> = system.incidents.iter().map(|i| i.title.clone()).collect();
                titles.sort();
                (system.name.clone(), system.status.clone(), titles)
            })
            .collect();
        Some((self.overall.clone(), systems))
    }
}

/// The content of a state: the overall word and, per system, its name, condition and the titles of its
/// incidents.
type Core = (String, Vec<(String, String, Vec<String>)>);

/// What was said last: the overall word of CIG and when it was asked (seconds).
#[derive(Clone, Debug, PartialEq)]
pub struct Overall {
    pub word: String,
    pub fetched: f64,
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// A number of the file, whole or with a fraction.
fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(text) => text.parse().ok(),
        Value::Float(n) => Some(*n),
        _ => None,
    }
}

fn text_of(map: &Map, key: &str) -> String {
    field(map, key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// `_timestamp`: `2026-08-26 14:15:00 +0000 UTC` (seconds optional) as seconds, read as UTC; `None` when it
/// cannot be read.
fn timestamp(raw: &str) -> Option<i64> {
    static STAMP: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2})(?::(\d{2}))?").expect("pattern"));
    let found = STAMP.captures(raw.trim())?;
    let second = found.get(3).map_or("00", |m| m.as_str());
    crate::logtime::seconds(&format!("{}T{}:{second}", &found[1], &found[2]))
}

fn timestamp_of(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_str).and_then(timestamp)
}

static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<!--.*?-->").expect("pattern"));
static BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").expect("pattern"));
static PARAGRAPH_END: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)</p\s*>").expect("pattern"));
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").expect("pattern"));
static STRONG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<(?:strong|b)\s*>(.*?)</(?:strong|b)\s*>").expect("pattern"));

/// The lines of the text of an incident (HTML), each with whether it stands highlighted throughout
/// (`_text_from_html`): a line is bold only when all its visible text lies inside `<strong>`/`<b>`.
pub fn text_from_html(raw: &str) -> Vec<(String, bool)> {
    let text = COMMENT.replace_all(raw, "");
    let text = BREAK.replace_all(&text, "\n");
    let text = PARAGRAPH_END.replace_all(&text, "\n\n");
    let mut lines = Vec::new();
    for piece in text.split('\n') {
        if crate::text::py_strip(piece).is_empty() {
            continue;
        }
        let without_tags = crate::text::py_strip(&TAG.replace_all(piece, "")).to_string();
        let highlight: Vec<String> =
            STRONG.captures_iter(piece).map(|hit| TAG.replace_all(&hit[1], "").into_owned()).collect();
        let highlight = crate::text::py_strip(&highlight.join(" ")).to_string();
        let bold = !highlight.is_empty() && highlight == without_tags;
        let clean = crate::stock_fetch::html_unescape(&without_tags);
        if !clean.is_empty() {
            lines.push((clean, bold));
        }
    }
    lines
}

/// An incident as the page lists it (`_incident_short`).
fn incident_short(raw: &Map) -> Incident {
    Incident {
        title: text_of(raw, "title"),
        severity: crate::text::py_strip(&text_of(raw, "severity")).to_string(),
        affected: match field(raw, "affected") {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => Vec::new(),
        },
        begun: timestamp_of(field(raw, "createdAt")),
        resolved: timestamp_of(field(raw, "resolvedAt")),
        file: text_of(raw, "filename"),
        address: text_of(raw, "permalink"),
        lines: Vec::new(),
    }
}

// ------------------------------------------------------------------ The file

fn int_or_null(value: Option<i64>) -> Value {
    value.map_or(Value::Null, Value::int)
}

/// An incident in the form of 4.12 (`titel`, `schwere` …; `zeilen` only with a text).
fn incident_value(incident: &Incident, with_lines: bool) -> Value {
    let mut map = vec![
        ("titel".to_string(), Value::str(&incident.title)),
        ("schwere".to_string(), Value::str(&incident.severity)),
        ("betroffen".to_string(), Value::Array(incident.affected.iter().map(Value::str).collect())),
        ("begonnen".to_string(), int_or_null(incident.begun)),
        ("erledigt".to_string(), int_or_null(incident.resolved)),
        ("datei".to_string(), Value::str(&incident.file)),
        ("adresse".to_string(), Value::str(&incident.address)),
    ];
    if with_lines {
        let lines = incident
            .lines
            .iter()
            .map(|(text, bold)| Value::Array(vec![Value::str(text), Value::Bool(*bold)]))
            .collect();
        map.push(("zeilen".to_string(), Value::Array(lines)));
    }
    Value::Object(map)
}

fn incident_from(value: &Value) -> Option<Incident> {
    let map = value.as_object()?;
    let lines = match field(map, "zeilen") {
        // Older files keep plain strings.
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                Value::Array(pair) => {
                    Some((pair.first()?.as_str()?.to_string(), matches!(pair.get(1), Some(Value::Bool(true)))))
                }
                Value::Str(text) => Some((text.clone(), false)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Some(Incident {
        title: text_of(map, "titel"),
        severity: text_of(map, "schwere"),
        affected: match field(map, "betroffen") {
            Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            _ => Vec::new(),
        },
        begun: field(map, "begonnen").and_then(number).map(|n| n as i64),
        resolved: field(map, "erledigt").and_then(number).map(|n| n as i64),
        file: text_of(map, "datei"),
        address: text_of(map, "adresse"),
        lines,
    })
}

fn state_value(state: &State) -> Value {
    let systems = state
        .systems
        .iter()
        .map(|system| {
            Value::Object(vec![
                ("name".into(), Value::str(&system.name)),
                ("status".into(), Value::str(&system.status)),
                ("ampel".into(), Value::str(&system.light)),
                ("farbe".into(), Value::str(&system.colour)),
                ("meldungen".into(), Value::Array(system.incidents.iter().map(|i| incident_value(i, false)).collect())),
            ])
        })
        .collect();
    Value::Object(vec![
        ("gesamt".into(), Value::str(&state.overall)),
        ("systeme".into(), Value::Array(systems)),
        ("geholt".into(), Value::Float(state.fetched)),
        ("stand".into(), int_or_null(state.stand)),
        ("quelle".into(), Value::str(&state.source)),
    ])
}

fn state_from(value: Option<&Value>) -> State {
    let Some(map) = value.and_then(Value::as_object) else { return State::default() };
    let systems = match field(map, "systeme") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_object)
            .map(|system| System {
                name: text_of(system, "name"),
                status: text_of(system, "status"),
                light: text_of(system, "ampel"),
                colour: text_of(system, "farbe"),
                incidents: match field(system, "meldungen") {
                    Some(Value::Array(items)) => items.iter().filter_map(incident_from).collect(),
                    _ => Vec::new(),
                },
            })
            .collect(),
        _ => Vec::new(),
    };
    State {
        overall: text_of(map, "gesamt"),
        systems,
        fetched: field(map, "geholt").and_then(number).unwrap_or(0.0),
        stand: field(map, "stand").and_then(number).map(|n| n as i64),
        source: text_of(map, "quelle"),
        no_net: false,
    }
}

/// The file kept; empty when there is none or it cannot be read.
fn read_file(paths: &Paths) -> Map {
    match jsonio::read_json(&paths.app_file(CACHE)) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// Writes the file kept; a write that fails costs only the next start a fetch.
fn write_file(paths: &Paths, data: Map) {
    let target = paths.app_file(CACHE);
    let folder = paths.host.flavor.dirname(&target);
    if !folder.is_empty() {
        let _ = jsonio::make_dirs(&folder);
    }
    let _ =
        jsonio::write_text_atomic(&target, &jsonio::dumps(&Value::Object(data), None, false), jsonio::Newline::Keep);
}

/// The state kept, without asking the page (`stored_state`); empty when nothing was ever fetched.
pub fn stored_state(paths: &Paths) -> State {
    state_from(field(&read_file(paths), "lage"))
}

/// The state kept as the overall word (`stored_state`), without asking the net; `None` when nothing was ever
/// fetched.
pub fn stored(paths: &Paths) -> Option<Overall> {
    let data = read_file(paths);
    let state = field(&data, "lage").and_then(Value::as_object)?;
    let word = crate::text::py_strip(&text_of(state, "gesamt")).to_string();
    let fetched = field(state, "geholt").and_then(number).unwrap_or(0.0);
    (!word.is_empty()).then_some(Overall { word, fetched })
}

// ------------------------------------------------------------------ Asking

/// The state of an answer of the page (`index.json`), asked at `now`.
fn read_answer(body: &[u8], now: f64) -> Option<State> {
    let Ok(Value::Object(data)) = jsonio::loads(&String::from_utf8_lossy(body)) else { return None };
    let colour_keys =
        [("ok", "colorOk"), ("gestoert", "colorDisrupted"), ("aus", "colorDown"), ("hinweis", "colorNotice")];
    let colour = |light: &str| {
        let key = colour_keys.iter().find(|(l, _)| *l == light).map_or("", |(_, k)| *k);
        Some(text_of(&data, key)).filter(|c| !c.is_empty()).unwrap_or_else(|| default_colour(light).to_string())
    };
    let systems = match field(&data, "systems") {
        Some(Value::Array(systems)) => systems
            .iter()
            .filter_map(Value::as_object)
            .map(|system| {
                let condition = crate::text::py_strip(&text_of(system, "status")).to_string();
                let lamp = light(&condition);
                System {
                    name: Some(text_of(system, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| "?".to_string()),
                    status: condition,
                    light: lamp.to_string(),
                    colour: colour(lamp),
                    incidents: match field(system, "unresolvedIssues") {
                        Some(Value::Array(items)) => {
                            items.iter().filter_map(Value::as_object).map(incident_short).collect()
                        }
                        _ => Vec::new(),
                    },
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    Some(State {
        overall: crate::text::py_strip(&text_of(&data, "summaryStatus")).to_string(),
        systems,
        fetched: now,
        stand: timestamp(&format!("{} {}", text_of(&data, "buildDate"), text_of(&data, "buildTime"))),
        source: format!("{BASE}/"),
        no_net: false,
    })
}

/// The state of an answer of the page in the form of 4.12, asked at `now`.
pub fn state_of(body: &[u8], now: f64) -> Option<Value> {
    read_answer(body, now).map(|state| state_value(&state))
}

/// Sets `key` of the file to `value`.
fn put(data: &mut Map, key: &str, value: Value) {
    jsonio::map_set(data, key, value);
}

/// The state of all systems (`state`): the one kept while it is younger than `limit` seconds (`None`: five
/// minutes) unless `force`; else the page is asked, with the tag of the last answer unless `force`. When the page
/// cannot be asked the state kept stands, marked [`State::no_net`]; with `SC_BP_NO_NET` nothing is asked and the
/// state kept counts as just looked at, as in 4.12.
pub fn state(paths: &Paths, net: &dyn Net, now: f64, force: bool, limit: Option<f64>) -> State {
    let mut data = read_file(paths);
    let old = state_from(field(&data, "lage"));
    let has_old = field(&data, "lage").and_then(Value::as_object).is_some_and(|m| !m.is_empty());
    let limit = limit.unwrap_or(FRESH_SECONDS);
    if has_old && now - old.fetched < limit && !force {
        return old;
    }
    let etag = field(&data, "etag").and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
    let asked_with = if force { None } else { etag };
    let answer = if crate::net::offline_in(&paths.host) {
        Ok(Fresh::Unchanged)
    } else {
        net.get_if_changed(&format!("{BASE}/index.json"), asked_with.as_deref())
    };
    let (body, tag) = match answer {
        Ok(Fresh::Body(body, tag)) => (body, tag.or(asked_with)),
        Ok(Fresh::Unchanged) => {
            if has_old {
                let mut kept = old;
                kept.fetched = now;
                put(&mut data, "lage", state_value(&kept));
                write_file(paths, data);
                return kept;
            }
            return old;
        }
        Err(_) => return State { no_net: true, ..old },
    };
    let Some(fresh) = read_answer(&body, now) else { return State { no_net: true, ..old } };
    put(&mut data, "etag", tag.map_or(Value::Null, Value::str));
    put(&mut data, "lage", state_value(&fresh));
    write_file(paths, data);
    fresh
}

/// A look whether something changed, for the minute of the open page (`ask`): asks with the tag, and says
/// whether the content differs from the state kept before (the time asked does not count).
pub fn ask(paths: &Paths, net: &dyn Net, now: f64) -> (State, bool) {
    let before = stored_state(paths);
    let fresh = state(paths, net, now, false, Some(0.0));
    let changed = fresh.core() != before.core();
    (fresh, changed)
}

/// The state, asked again when the one kept is older than five minutes: the overall word, or `None` when nothing
/// was ever fetched and the page cannot be asked. For the tile of the overview.
pub fn refresh(paths: &Paths, net: &dyn Net, now: f64) -> Option<Overall> {
    if crate::net::offline_in(&paths.host) {
        return stored(paths);
    }
    let _ = state(paths, net, now, false, None);
    stored(paths)
}

/// The JSON of an address of the page; `None` when it cannot be had or read, or the net is switched off.
fn fetch_json(paths: &Paths, net: &dyn Net, path: &str) -> Option<Map> {
    if crate::net::offline_in(&paths.host) {
        return None;
    }
    let body = net.get(&format!("{BASE}{path}")).ok()?;
    match jsonio::loads(&String::from_utf8_lossy(&body)) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// One incident in full, with its text (`incident`): `file` is its file of the list (`….md`), the rest names
/// its folder under `/issues/`.
pub fn incident(paths: &Paths, net: &dyn Net, file: &str) -> Option<Incident> {
    let name = crate::text::py_strip(file);
    let name = name.strip_suffix(".md").unwrap_or(name);
    if name.is_empty() {
        return None;
    }
    let data = fetch_json(paths, net, &format!("/issues/{name}/index.json"))?;
    if data.is_empty() {
        return None;
    }
    let mut found = incident_short(&data);
    found.lines = text_from_html(&text_of(&data, "body"));
    Some(found)
}

/// The latest incidents, newest first (`history`).
pub fn history(paths: &Paths, net: &dyn Net, at_most: usize) -> Vec<Incident> {
    let Some(data) = fetch_json(paths, net, "/issues/index.json") else { return Vec::new() };
    match field(&data, "pages") {
        Some(Value::Array(pages)) => {
            pages.iter().take(at_most).filter_map(Value::as_object).map(incident_short).collect()
        }
        _ => Vec::new(),
    }
}

/// The incidents of the last `months` months in full, resolved ones too, at most `at_most` (`messages`). A
/// resolved incident is fetched once and kept in the file (`volltexte`); an open one is fetched again, as it
/// gets further lines.
pub fn messages(paths: &Paths, net: &dyn Net, now: f64, months: i64, at_most: usize) -> Vec<Incident> {
    let limit = now as i64 - months * 30 * 86_400;
    let mut data = read_file(paths);
    let mut kept: Map = match field(&data, "volltexte") {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    };
    let (mut result, mut fetched) = (Vec::new(), false);
    for short in history(paths, net, HISTORY_LOOKED_AT) {
        if short.begun.is_some_and(|when| when != 0 && when < limit) {
            break;
        }
        let full = match field(&kept, &short.file).and_then(incident_from) {
            Some(full) => Some(full),
            None => {
                let full = incident(paths, net, &short.file);
                if let Some(full) = full.as_ref().filter(|full| full.resolved.is_some()) {
                    jsonio::map_set(&mut kept, &short.file, incident_value(full, true));
                    fetched = true;
                }
                full
            }
        };
        if let Some(full) = full {
            result.push(full);
        }
        if result.len() >= at_most {
            break;
        }
    }
    if fetched {
        put(&mut data, "volltexte", Value::Object(kept));
        write_file(paths, data);
    }
    result
}

/// How long ago `stamp` was, as the status page writes it („gerade eben“, „vor 7 Std.“ …); a dash without a
/// time (`_relative_time`).
pub fn relative_time(stamp: Option<f64>, now: f64) -> String {
    match stamp.filter(|s| *s != 0.0) {
        Some(stamp) => crate::language::relative_age(now - stamp),
        None => "—".to_string(),
    }
}

#[cfg(test)]
mod tests;
