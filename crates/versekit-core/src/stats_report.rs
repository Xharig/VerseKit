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

//! What the six statistics pages show: the sums, rankings and the heat map
//! worked out of `statistik.json` (the sessions), `spielzeit.json` (the
//! periods) and `auftragslog.json` (the contract names). The counting half of
//! `play_stats.py`: `summary`, `ships`, `missions`, `quantum`, `stability`,
//! the readable places and the export.
//!
//! ⚠ A session of another account does not count, like with the blueprints;
//! a session without a known account does (`counts_for`). ⚠ Nothing that the
//! log does not prove stands here: no injuries, no zones.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::formats::{self, LocalTime};
use crate::jsonio::{self, Map, Value};
use crate::language::Lang;
use crate::logsource::counts_for;
use crate::logtime::seconds;
use crate::mission_log::{self, get, truthy};
use crate::paths::Paths;
use crate::play_stats;
use crate::playtime;

/// The ships, weapons and places a page lists at most (`_top` default).
pub const TOP: usize = 5;
/// The places and starting points of the quantum page.
pub const TOP_PLACES: usize = 6;
/// The reasons of lost connections.
pub const TOP_REASONS: usize = 8;
/// The latest contracts and sessions in the tables.
pub const RECENT: usize = 8;

/// The weapon places in the order of the page: back, side, hand.
pub const WEAPON_SLOTS: [&str; 3] = ["ruecken", "seite", "hand"];

/// A number as Python's `get(key, 0)` hands it out.
fn whole(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Int(text)) => text.parse().unwrap_or(0),
        Some(Value::Float(x)) if x.is_finite() => *x as i64,
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

fn int(entry: &Map, key: &str) -> i64 {
    whole(get(entry, key))
}

fn strings(entry: &Map, key: &str) -> Vec<String> {
    match get(entry, key) {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// `{name: count}` of an object, in the order of the file.
fn count_pairs(value: Option<&Value>) -> Vec<(String, i64)> {
    match value {
        Some(Value::Object(map)) => map.iter().map(|(name, n)| (name.clone(), whole(Some(n)))).collect(),
        _ => Vec::new(),
    }
}

/// The joins `[[moment, shard], …]` of an entry; a pair that is not of that shape is left out.
fn joins(value: Option<&Value>) -> Vec<crate::shard::Join> {
    let Some(Value::Array(items)) = value else { return Vec::new() };
    items
        .iter()
        .filter_map(|item| match item {
            Value::Array(pair) => match pair.as_slice() {
                [at, Value::Str(id)] => Some(crate::shard::Join {
                    at: matches!(at, Value::Int(_)).then(|| whole(Some(at))),
                    shard: crate::shard::Shard { id: id.clone() },
                }),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// One evaluated session as `statistik.json` keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub from: i64,
    pub to: i64,
    pub clean: bool,
    pub account: String,
    pub contracts: i64,
    pub failed: i64,
    pub abandoned: i64,
    pub jumps: i64,
    /// The ships steered, once each.
    pub ships: Vec<String>,
    /// The ships thrown out of, once per loss.
    pub lost: Vec<String>,
    /// The moments of the deaths, in seconds.
    pub deaths: Vec<i64>,
    /// Slot → (weapon, pieces).
    pub weapons: Vec<(String, Vec<(String, i64)>)>,
    pub selections: i64,
    pub targets: Vec<(String, i64)>,
    pub starts: Vec<(String, i64)>,
    pub crashed: bool,
    /// Reason → how often.
    pub drops: Vec<(String, i64)>,
    /// The joins of a shard, in the order of the log (none in an entry of an older version).
    pub shards: Vec<crate::shard::Join>,
    /// Which of the exported counts the entry holds at all (an older format
    /// lacks some; the export tells none from zero).
    pub present: Present,
}

/// The counts of an entry that the export writes, and whether the entry has them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Present {
    pub contracts: bool,
    pub failed: bool,
    pub jumps: bool,
    pub deaths: bool,
}

impl Present {
    /// An entry that holds all four.
    pub const ALL: Present = Present { contracts: true, failed: true, jumps: true, deaths: true };
}

impl Session {
    pub fn of(entry: &Map) -> Session {
        let weapons = match get(entry, "waffen") {
            Some(Value::Object(slots)) => {
                slots.iter().map(|(slot, found)| (slot.clone(), count_pairs(Some(found)))).collect()
            }
            _ => Vec::new(),
        };
        let deaths = match get(entry, "tode") {
            Some(Value::Array(items)) => items.iter().map(|v| whole(Some(v))).collect(),
            _ => Vec::new(),
        };
        Session {
            from: int(entry, "von"),
            to: int(entry, "bis"),
            clean: get(entry, "sauber").is_some_and(truthy),
            account: get(entry, "account").and_then(Value::as_str).unwrap_or("").to_string(),
            contracts: int(entry, "auftraege"),
            failed: int(entry, "fehlgeschlagen"),
            abandoned: int(entry, "abgebrochen"),
            jumps: int(entry, "spruenge"),
            ships: strings(entry, "schiffe"),
            lost: strings(entry, "verloren"),
            deaths,
            weapons,
            selections: int(entry, "zielwahlen"),
            targets: count_pairs(get(entry, "ziele")),
            starts: count_pairs(get(entry, "starts")),
            crashed: get(entry, "absturz").is_some_and(truthy),
            drops: count_pairs(get(entry, "abbrueche")),
            shards: joins(get(entry, "shards")),
            present: Present {
                contracts: get(entry, "auftraege").is_some(),
                failed: get(entry, "fehlgeschlagen").is_some(),
                jumps: get(entry, "spruenge").is_some(),
                deaths: get(entry, "tode").is_some(),
            },
        }
    }

    /// How the session ended: a crash beats a clean end, because after a
    /// graphics crash the game often still writes `Quit` (`end_of`).
    pub fn end(&self) -> End {
        if self.crashed {
            End::Crash
        } else if self.clean {
            End::Clean
        } else {
            End::Open
        }
    }
}

/// How a session ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Clean,
    Crash,
    Open,
}

/// All evaluated sessions of the data folder, in the order of the file.
pub fn load_sessions(paths: &Paths) -> Vec<Session> {
    let data = play_stats::load(paths);
    match get(&data, "sitzungen") {
        Some(Value::Object(map)) => map.iter().filter_map(|(_, v)| v.as_object()).map(Session::of).collect(),
        _ => Vec::new(),
    }
}

/// The sessions that count for the own account.
pub fn counted<'a>(sessions: &'a [Session], own: Option<&str>) -> Vec<&'a Session> {
    sessions.iter().filter(|s| counts_for(own, Some(&s.account))).collect()
}

/// The biggest entries, biggest first.
///
/// ⚠ A tie goes by name, so the order does not jump with every opening.
pub fn top(counter: &HashMap<String, i64>, limit: usize) -> Vec<(String, i64)> {
    let mut list: Vec<(String, i64)> = counter.iter().map(|(k, v)| (k.clone(), *v)).collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list.truncate(limit);
    list
}

fn add(counter: &mut HashMap<String, i64>, key: &str, n: i64) {
    *counter.entry(key.to_string()).or_insert(0) += n;
}

// ---------------------------------------------------------------------------
// Time of day
// ---------------------------------------------------------------------------

/// Monday 0 … Sunday 6 (days since 1970-01-01, a Thursday).
pub fn weekday(t: &LocalTime) -> usize {
    let (y, m, d) = (t.year - i64::from(t.month <= 2), i64::from(t.month), i64::from(t.day));
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    (days + 3).rem_euclid(7) as usize
}

/// Seconds played per weekday (Monday 0) and hour, in local time.
pub type Heat = [[i64; 24]; 7];

/// 7 × 24 seconds in local time, Monday = 0.
///
/// ⚠ At the hour borders it is **split**: a session from 23:30 to 00:30
/// counts half an hour in each of the two hours and in both days, not all in
/// the hour it began.
pub fn heat_map(spans: &[(i64, i64)]) -> Heat {
    let mut grid = [[0; 24]; 7];
    for &(start, end) in spans {
        let mut cursor = start;
        while cursor < end {
            let Some(local) = formats::local_time(cursor) else { break };
            let to_next_hour = 3600 - i64::from(local.minute) * 60 - i64::from(local.second);
            let piece = end.min(cursor + to_next_hour) - cursor;
            grid[weekday(&local)][local.hour as usize] += piece;
            cursor += piece.max(1);
        }
    }
    grid
}

/// `2026-10-21` of the local day of `seconds`, as the language writes a date:
/// `21.10.2026` in German, unchanged in English.
pub fn local_day(lang: Lang, seconds: i64) -> String {
    let Some(t) = formats::local_time(seconds) else { return String::new() };
    match lang {
        Lang::De => formats::strftime("%d.%m.%Y", &t),
        Lang::En => formats::strftime("%Y-%m-%d", &t),
    }
}

/// `21.10.2026, 19:05`.
pub fn local_moment(lang: Lang, seconds: i64) -> String {
    let Some(t) = formats::local_time(seconds) else { return String::new() };
    format!("{}, {}", local_day(lang, seconds), formats::strftime("%H:%M", &t))
}

/// A stamp of the game log (UTC, no zone) as it stands, without conversion:
/// the seconds, or `None` for a stamp that is none (`log_seconds`).
pub fn log_seconds(stamp: &str) -> Option<i64> {
    seconds(stamp)
}

/// A share as a whole percent, rounded like Python's `round` (half to even);
/// `0` without a whole.
pub fn percent(part: i64, whole: i64) -> i64 {
    if whole == 0 {
        return 0;
    }
    (100.0 * part as f64 / whole as f64).round_ties_even() as i64
}

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

/// Everything the overview shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub playtime: i64,
    pub sessions: i64,
    pub average: i64,
    pub longest: i64,
    pub contracts: i64,
    pub failed: i64,
    pub jumps: i64,
    pub deaths: i64,
    pub last_death: Option<i64>,
    pub lost: i64,
    pub most_lost: Option<String>,
    /// How many sessions are evaluated: the base of "per session". Not
    /// `sessions`: that one counts old times from `spielzeit.json` too, whose
    /// log was gone before it could be counted.
    pub recorded: i64,
    pub since: Option<i64>,
    pub heat: Heat,
}

/// The overview from the evaluated sessions (`stats`), the periods of
/// `spielzeit.json` (`spans`) and the session running now.
///
/// Periods whose session is evaluated and belongs to another account are
/// left out; one without an evaluation (the log is gone) counts for time and
/// heat map, its account is unknown and unknown counts.
pub fn summary(stats: &[Session], spans: &[(i64, i64)], running: Option<(i64, i64)>, own: Option<&str>) -> Summary {
    let by_start: HashMap<i64, &Session> = stats.iter().map(|s| (s.from, s)).collect();
    let mut all: Vec<(i64, i64)> = spans
        .iter()
        .filter(|(start, _)| by_start.get(start).is_none_or(|mine| counts_for(own, Some(&mine.account))))
        .copied()
        .collect();
    all.extend(running);
    let merged = playtime::merge_spans(&all);
    let total: i64 = merged.iter().map(|(start, end)| end.saturating_sub(*start)).sum();
    let mine = counted(stats, own);
    let sessions = merged.len() as i64;
    let mut lost: HashMap<String, i64> = HashMap::new();
    for session in &mine {
        for ship in &session.lost {
            add(&mut lost, ship, 1);
        }
    }
    let deaths: Vec<i64> = mine.iter().flat_map(|s| s.deaths.iter().copied()).collect();
    Summary {
        playtime: total,
        sessions,
        average: if sessions > 0 { total.div_euclid(sessions) } else { 0 },
        longest: merged.iter().map(|(start, end)| end.saturating_sub(*start)).max().unwrap_or(0),
        contracts: mine.iter().map(|s| s.contracts).sum(),
        failed: mine.iter().map(|s| s.failed).sum(),
        jumps: mine.iter().map(|s| s.jumps).sum(),
        deaths: deaths.len() as i64,
        last_death: deaths.iter().copied().max(),
        lost: lost.values().sum(),
        most_lost: top(&lost, 1).into_iter().next().map(|(name, _)| name),
        recorded: mine.len() as i64,
        since: merged.iter().map(|(start, _)| *start).min(),
        heat: heat_map(&merged),
    }
}

// ---------------------------------------------------------------------------
// Ships and equipment
// ---------------------------------------------------------------------------

/// The weapons of one place: the most carried and the sum of all.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponSlot {
    pub slot: &'static str,
    pub top: Vec<(String, i64)>,
    pub total: i64,
}

/// Ships and equipment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ships {
    /// How many different ships.
    pub ships: usize,
    /// The ship steered in most sessions, and in how many.
    pub most: Option<(String, i64)>,
    /// In how many sessions a ship was steered at all.
    pub sessions_with_ship: i64,
    pub used: Vec<(String, i64)>,
    pub lost: Vec<(String, i64)>,
    pub lost_total: i64,
    pub weapons: Vec<WeaponSlot>,
}

/// What was flown with, lost and carried. A ship counts once per session.
pub fn ships(stats: &[Session], own: Option<&str>) -> Ships {
    let mine = counted(stats, own);
    let (mut used, mut lost): (HashMap<String, i64>, HashMap<String, i64>) = (HashMap::new(), HashMap::new());
    let mut weapons: Vec<(&'static str, HashMap<String, i64>)> =
        WEAPON_SLOTS.iter().map(|slot| (*slot, HashMap::new())).collect();
    let mut flown = 0;
    for session in &mine {
        if !session.ships.is_empty() {
            flown += 1;
        }
        for ship in &session.ships {
            add(&mut used, ship, 1);
        }
        for ship in &session.lost {
            add(&mut lost, ship, 1);
        }
        for (slot, found) in &session.weapons {
            if let Some((_, bucket)) = weapons.iter_mut().find(|(name, _)| name == slot) {
                for (class, n) in found {
                    add(bucket, class, *n);
                }
            }
        }
    }
    Ships {
        ships: used.len(),
        most: top(&used, 1).into_iter().next(),
        sessions_with_ship: flown,
        used: top(&used, TOP),
        lost: top(&lost, TOP),
        lost_total: lost.values().sum(),
        weapons: weapons
            .into_iter()
            .map(|(slot, found)| WeaponSlot { slot, top: top(&found, TOP), total: found.values().sum() })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Contracts
// ---------------------------------------------------------------------------

/// A contract of the table: name, stamp (`YYYY-MM-DDThh:mm:ss`) and state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recent {
    pub name: String,
    pub stamp: String,
    pub state: String,
}

/// How the contracts ended.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Missions {
    pub completed: i64,
    pub failed: i64,
    pub abandoned: i64,
    pub frequent: Vec<(String, i64)>,
    pub latest: Vec<Recent>,
}

fn text(entry: &Map, key: &str) -> String {
    get(entry, key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Outcomes from the statistics, names from the contract log.
///
/// ⚠ The sums at the top come from `<EndMission>` (the own ones only, one per
/// contract), the same as on the overview. Names are not there; they come
/// from the contract log, which knows only contracts whose title shows in the
/// log. So the lists are no sums.
pub fn missions(stats: &[Session], log: &[Map], own: Option<&str>) -> Missions {
    let mine = counted(stats, own);
    let ended: Vec<&Map> = log.iter().filter(|e| text(e, "zustand") != mission_log::RUNNING).collect();
    let mut names: HashMap<String, i64> = HashMap::new();
    for entry in &ended {
        let name = text(entry, "name");
        if !name.is_empty() {
            add(&mut names, &name, 1);
        }
    }
    let when = |e: &Map| {
        let until = text(e, "bis");
        if until.is_empty() { text(e, "wann") } else { until }
    };
    let mut latest = ended.clone();
    // Stable, newest first: equal stamps keep the order of the log.
    latest.sort_by_key(|entry| std::cmp::Reverse(when(entry)));
    latest.truncate(RECENT);
    Missions {
        completed: mine.iter().map(|s| s.contracts).sum(),
        failed: mine.iter().map(|s| s.failed).sum(),
        abandoned: mine.iter().map(|s| s.abandoned).sum(),
        frequent: top(&names, TOP),
        latest: latest
            .into_iter()
            .map(|e| Recent { name: text(e, "name"), stamp: when(e), state: text(e, "zustand") })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Quantum travel
// ---------------------------------------------------------------------------

/// Quantum travel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quantum {
    pub jumps: i64,
    pub selections: i64,
    pub targets: Vec<(String, i64)>,
    pub starts: Vec<(String, i64)>,
}

/// Targets chosen, arrivals, the most frequent targets and starting points.
pub fn quantum(stats: &[Session], own: Option<&str>) -> Quantum {
    let mine = counted(stats, own);
    let (mut targets, mut starts): (HashMap<String, i64>, HashMap<String, i64>) = (HashMap::new(), HashMap::new());
    for session in &mine {
        for (place, n) in &session.targets {
            add(&mut targets, place, *n);
        }
        for (place, n) in &session.starts {
            add(&mut starts, place, *n);
        }
    }
    Quantum {
        jumps: mine.iter().map(|s| s.jumps).sum(),
        selections: mine.iter().map(|s| s.selections).sum(),
        targets: top(&targets, TOP_PLACES),
        starts: top(&starts, TOP_PLACES),
    }
}

// ---------------------------------------------------------------------------
// Stability
// ---------------------------------------------------------------------------

/// A session of the table: start, duration, end and lost connections.
#[derive(Clone, Debug, PartialEq)]
pub struct Last {
    pub start: i64,
    pub duration: i64,
    pub end: End,
    pub drops: i64,
}

/// How sessions ended and why the connection broke off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stability {
    pub sessions: i64,
    pub clean: i64,
    pub crashed: i64,
    pub open: i64,
    pub reasons: Vec<(String, i64)>,
    pub drops: i64,
    pub latest: Vec<Last>,
    /// The start of the newest session and its joins of a shard; `None` without a session.
    pub shards: Option<(i64, Vec<crate::shard::Join>)>,
}

pub fn stability(stats: &[Session], own: Option<&str>) -> Stability {
    let mine = counted(stats, own);
    let crashed = mine.iter().filter(|s| s.crashed).count() as i64;
    let clean = mine.iter().filter(|s| s.clean && !s.crashed).count() as i64;
    let mut reasons: HashMap<String, i64> = HashMap::new();
    for session in &mine {
        for (reason, n) in &session.drops {
            add(&mut reasons, reason, *n);
        }
    }
    let mut latest = mine.clone();
    latest.sort_by_key(|session| std::cmp::Reverse(session.from));
    let shards = latest.first().map(|newest| (newest.from, newest.shards.clone()));
    latest.truncate(RECENT);
    Stability {
        shards,
        sessions: mine.len() as i64,
        clean,
        crashed,
        open: mine.len() as i64 - clean - crashed,
        reasons: top(&reasons, TOP_REASONS),
        drops: reasons.values().sum(),
        latest: latest
            .into_iter()
            .map(|s| Last {
                start: s.from,
                duration: s.to.saturating_sub(s.from),
                end: s.end(),
                drops: s.drops.iter().map(|(_, n)| n).sum(),
            })
            .collect(),
    }
}

/// The text key of a known reason for a broken connection; an unknown one
/// stays as the game writes it: better raw than translated wrongly.
pub fn reason_key(reason: &str) -> Option<&'static str> {
    REASONS.iter().find(|(known, _)| *known == reason).map(|(_, key)| *key)
}

/// The reasons the game gives for a broken connection, as it writes them, and
/// the text key of each.
pub const REASONS: [(&str, &str); 6] = [
    ("Nub destroyed", "s_sb_r_nub"),
    ("Remote Disconnect - Player requested disconnect", "s_sb_r_selbst"),
    ("DisconnectCmd: disconnect light ExitToMenu", "s_sb_r_menue"),
    ("Remote Disconnect - player inactive", "s_sb_r_inaktiv"),
    ("DGS disconnecting all channels before game shutdown", "s_sb_r_server"),
    ("Remote Disconnect - Dup Login", "s_sb_r_doppelt"),
];

// ---------------------------------------------------------------------------
// Readable places and names
// ---------------------------------------------------------------------------

/// Targets that are no place: the own waypoint, a party member, the beacon of
/// a contract, any rest stop. Measured: together every fifth choice.
pub fn place_label_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "NavPoint_Dynamic" => "s_sq_p_wegpunkt",
        "PartyMemberMarker" => "s_sq_p_party",
        "MISSION_QT" => "s_sq_p_auftrag",
        "ObjectContainer_RestStop" => "s_sq_p_rast",
        _ => return None,
    })
}

/// The file with the place names read from the `global.ini` once.
pub const PLACE_FILE: &str = "orte-original.json";
/// The file with the names of ships and items from the `global.ini`.
pub const NAME_FILE: &str = "namen-original.json";

/// The short names of the Stanton planets in the names of rest stops.
fn body_short(short: &str) -> &str {
    match short {
        "hur" => "stanton1",
        "cru" => "stanton2",
        "arc" => "stanton3",
        "mic" => "stanton4",
        "stan" => "stanton",
        other => other,
    }
}

/// Cities and stations whose containers follow no pattern: names from the
/// game, the same in every language.
fn city_name(low: &str) -> Option<&'static str> {
    Some(match low {
        "levski_all-001" => "Levski",
        "grimhex_oc" => "GrimHEX",
        "newbabbage_loc" => "New Babbage",
        "orison_loc" => "Orison",
        "area18_city_objectcontainer" => "Area 18",
        "lorville_loc" => "Lorville",
        _ => return None,
    })
}

static RS_ORBIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^rs_ext_([a-z]+\d*)[-_]leo\d*$").expect("fester Ausdruck"));
static RS_POINT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^rs_ext_([a-z]+\d+)_(l\d)$").expect("fester Ausdruck"));
static RS_JUMP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^rs_ext_([a-z]+)-([a-z]+)_jp\d*$").expect("fester Ausdruck"));
static RR_POINT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^rr_s(\d)_(l\d)$").expect("fester Ausdruck"));
static OOC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^ooc_.*_([A-Za-z][A-Za-z0-9]+)$").expect("fester Ausdruck"));
static SYSTEM_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+System$").expect("fester Ausdruck"));

/// Python's `str.capitalize`: the first letter up, the rest down.
fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map_or_else(String::new, |first| first.to_uppercase().chain(chars.flat_map(char::to_lowercase)).collect())
}

/// A target made readable: the text key (with the name for its `{0}`, or
/// without a name for a plain label) and the name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub text_key: Option<&'static str>,
    pub name: String,
}

/// The names the places are looked up in: `{key in lower case: name}`.
pub type Names = HashMap<String, String>;

/// Makes a target readable, e.g. `Raststation im Orbit von Terminus`.
///
/// ⚠ The places stand only as keys in the log, and for most of them no
/// display name exists. Their **names follow a fixed pattern** though, and
/// the parts of it (planets, Lagrange points, systems) stand in the
/// `global.ini`:
///
/// ```text
/// rs_ext_pyro6_leo        rest stop in the orbit of  pyro6 = Terminus
/// rs_ext_pyro2_l4         rest stop at               pyro2_l4 = PYR2 L4
/// rs_ext_stan-magnus_jp1  rest stop at the jump point Stanton – Magnus
/// RR_S1_L2                rest stop at               stanton1_l2 = HUR L2
/// OOC_Stanton_2b_Daymar   the last part is the name: Daymar
/// ```
///
/// What follows no pattern (bunkers, hideouts) stays as the game writes it.
pub fn place_parts(key: &str, names: &Names) -> Place {
    if let Some(label) = place_label_key(key) {
        return Place { text_key: Some(label), name: String::new() };
    }
    let low = key.to_lowercase();
    let body = |short: &str| {
        let short = body_short(short);
        names.get(short).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| capitalize(short))
    };
    if let Some(city) = city_name(&low) {
        return Place { text_key: None, name: city.to_string() };
    }
    if let Some(found) = RS_ORBIT.captures(&low) {
        return Place { text_key: Some("s_sq_o_orbit"), name: body(&found[1]) };
    }
    if let Some(found) = RS_POINT.captures(&low) {
        let name = names
            .get(&format!("{}_{}", &found[1], &found[2]))
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| format!("{} {}", body(&found[1]), found[2].to_uppercase()));
        return Place { text_key: Some("s_sq_o_punkt"), name };
    }
    if let Some(found) = RS_JUMP.captures(&low) {
        // `Magnus System` is the name of the system in the `global.ini`: in
        // the jump point `Stanton – Magnus` the word would be said twice.
        let end = |part: &str| SYSTEM_WORD.replace(&body(part), "").into_owned();
        return Place { text_key: Some("s_sq_o_sprung"), name: format!("{} – {}", end(&found[1]), end(&found[2])) };
    }
    if let Some(found) = RR_POINT.captures(&low) {
        let name = names
            .get(&format!("stanton{}_{}", &found[1], &found[2]))
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| key.to_string());
        return Place { text_key: Some("s_sq_o_punkt"), name };
    }
    if let Some(name) = names.get(&low).filter(|n| !n.is_empty()) {
        return Place { text_key: None, name: name.clone() };
    }
    if let Some(found) = OOC.captures(key) {
        return Place { text_key: None, name: found[1].to_string() };
    }
    Place { text_key: None, name: key.to_string() }
}

/// The saved names of a file as `{key in lower case: text}`; nothing for a
/// file that is missing or not an object.
pub fn saved_names(paths: &Paths, file: &str) -> Names {
    match jsonio::read_json(&paths.app_file(file)) {
        Ok(Value::Object(map)) => map
            .into_iter()
            .filter_map(|(key, value)| match value {
                Value::Str(text) => Some((key.to_lowercase(), text)),
                _ => None,
            })
            .collect(),
        _ => Names::new(),
    }
}

/// `AEGS_Sabre` → `Aegis Sabre`, `behr_lmg_ballistic_01` → `FS-9 LMG`.
/// Without a name table the key stays, only more readable.
pub fn display_name(class: &str, vehicle: bool, names: &Names) -> String {
    let key = format!("{}{}", if vehicle { "vehicle_name" } else { "item_name" }, class.to_lowercase());
    names.get(&key).filter(|n| !n.is_empty()).cloned().unwrap_or_else(|| class.replace('_', " "))
}

/// The names the pages of one report show: of ships and weapons, and of the
/// parts of the places.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReportNames {
    pub things: Names,
    pub places: Names,
}

/// Looks up only the names the sums ask for in the saved name files (the
/// ship table has a million bytes; the pages need a few dozen of its lines).
/// A file that is not there gives no names: the keys stand then, more readable.
pub fn report_names(paths: &Paths, ships: &Ships, summary: &Summary, quantum: &Quantum) -> ReportNames {
    let mut things: Vec<String> = Vec::new();
    for (class, _) in ships.used.iter().chain(&ships.lost).chain(ships.most.iter()) {
        things.push(format!("vehicle_name{}", class.to_lowercase()));
    }
    things.extend(summary.most_lost.iter().map(|class| format!("vehicle_name{}", class.to_lowercase())));
    for slot in &ships.weapons {
        things.extend(slot.top.iter().map(|(class, _)| format!("item_name{}", class.to_lowercase())));
    }
    let places: Vec<String> = quantum.targets.iter().flat_map(|(place, _)| place_name_keys(place)).collect();
    let pick = |all: Names, wanted: Vec<String>| -> Names {
        wanted.into_iter().filter_map(|key| all.get(&key).map(|name| (key, name.clone()))).collect()
    };
    ReportNames {
        things: pick(saved_names(paths, NAME_FILE), things),
        places: pick(saved_names(paths, PLACE_FILE), places),
    }
}

/// The keys of the `global.ini` one target may need: the parts of its name.
pub fn place_name_keys(key: &str) -> Vec<String> {
    let low = key.to_lowercase();
    let mut out = Vec::new();
    let body = |short: &str| body_short(short).to_string();
    if let Some(found) = RS_ORBIT.captures(&low) {
        out.push(body(&found[1]));
    } else if let Some(found) = RS_POINT.captures(&low) {
        out.push(format!("{}_{}", &found[1], &found[2]));
        out.push(body(&found[1]));
    } else if let Some(found) = RS_JUMP.captures(&low) {
        out.push(body(&found[1]));
        out.push(body(&found[2]));
    } else if let Some(found) = RR_POINT.captures(&low) {
        out.push(format!("stanton{}_{}", &found[1], &found[2]));
    } else {
        out.push(low);
    }
    out
}

// ---------------------------------------------------------------------------
// The evaluation page
// ---------------------------------------------------------------------------

/// What the evaluation page tells: when, how much, how big.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Evaluation {
    /// The last run, or the one that found something (seconds since 1970).
    pub last: Option<f64>,
    pub sessions: usize,
    pub files: usize,
    /// Bytes of `statistik.json`, 0 without the file.
    pub size: u64,
}

/// `last_run` is the moment of the last look in this program, if any; the
/// file itself keeps only the last run with a find.
pub fn evaluation(paths: &Paths, last_run: Option<f64>) -> Evaluation {
    let data = play_stats::load(paths);
    let count = |key: &str| match get(&data, key) {
        Some(Value::Object(map)) => map.len(),
        _ => 0,
    };
    let stored = match get(&data, "stand") {
        Some(Value::Float(x)) => Some(*x),
        Some(Value::Int(text)) => text.parse().ok(),
        _ => None,
    };
    Evaluation {
        last: last_run.or(stored),
        sessions: count("sitzungen"),
        files: count("gelesen"),
        size: std::fs::metadata(paths.app_file(play_stats::FILE)).map_or(0, |m| m.len()),
    }
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

fn utc_stamp(seconds: i64) -> String {
    crate::export::utc_text(seconds, false)
}

fn optional(value: Option<i64>) -> Value {
    value.map_or(Value::Null, Value::int)
}

/// The statistics as a file to keep or to pass on (`export`).
///
/// ⚠ **Without the player's name.** The file is meant to be shared; who shows
/// it in Discord should not give away the account on the side.
pub fn export_value(
    stats: &[Session],
    spans: &[(i64, i64)],
    running: Option<(i64, i64)>,
    own: Option<&str>,
    version: &str,
    now: i64,
) -> Value {
    let data = summary(stats, spans, running, own);
    let by_start: HashMap<i64, &Session> = stats.iter().map(|s| (s.from, s)).collect();
    let rows = spans
        .iter()
        .map(|&(start, end)| {
            let mine = by_start.get(&start).copied();
            Value::Object(vec![
                ("von".into(), Value::str(utc_stamp(start))),
                ("bis".into(), Value::str(utc_stamp(end))),
                ("auftraege".into(), optional(mine.filter(|s| s.present.contracts).map(|s| s.contracts))),
                ("fehlgeschlagen".into(), optional(mine.filter(|s| s.present.failed).map(|s| s.failed))),
                ("spruenge".into(), optional(mine.filter(|s| s.present.jumps).map(|s| s.jumps))),
                ("tode".into(), optional(mine.filter(|s| s.present.deaths).map(|s| s.deaths.len() as i64))),
            ])
        })
        .collect();
    let heat =
        Value::Array(data.heat.iter().map(|row| Value::Array(row.iter().map(|n| Value::int(*n)).collect())).collect());
    let total = Value::Object(vec![
        ("spielzeit".into(), Value::int(data.playtime)),
        ("sitzungen".into(), Value::int(data.sessions)),
        ("schnitt".into(), Value::int(data.average)),
        ("laengste".into(), Value::int(data.longest)),
        ("auftraege".into(), Value::int(data.contracts)),
        ("fehlgeschlagen".into(), Value::int(data.failed)),
        ("spruenge".into(), Value::int(data.jumps)),
        ("tode".into(), Value::int(data.deaths)),
        ("letzter_tod".into(), optional(data.last_death)),
        ("verloren".into(), Value::int(data.lost)),
        ("meist_verloren".into(), data.most_lost.map_or(Value::Null, Value::str)),
        ("erfasst".into(), Value::int(data.recorded)),
        ("seit".into(), optional(data.since)),
    ]);
    Value::Object(vec![
        ("werkzeug".into(), Value::str("VerseKit")),
        ("version".into(), Value::str(version)),
        ("erstellt".into(), Value::str(utc_stamp(now))),
        ("gesamt".into(), total),
        ("waermekarte".into(), heat),
        ("sitzungen".into(), Value::Array(rows)),
    ])
}

/// Writes the export to `target`; false when it does not work (the reason is
/// recorded).
pub fn export(target: &str, value: &Value) -> bool {
    let text = jsonio::dumps(value, Some(1), false);
    match jsonio::write_text_atomic(target, &text, jsonio::Newline::Native) {
        Ok(()) => true,
        Err(e) => {
            crate::errors::record("play_stats.export", &e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(from: i64, f: impl FnOnce(&mut Session)) -> Session {
        let mut s = Session { from, to: from + 3600, account: "Xharig".into(), ..Session::default() };
        f(&mut s);
        s
    }

    #[test]
    fn weekdays_count_from_monday() {
        let at = |y, m, d| LocalTime { year: y, month: m, day: d, hour: 12, minute: 0, second: 0 };
        assert_eq!(weekday(&at(2026, 10, 21)), 2);
        assert_eq!(weekday(&at(2026, 10, 25)), 6);
        assert_eq!(weekday(&at(2024, 2, 29)), 3);
        assert_eq!(weekday(&at(2000, 1, 1)), 5);
    }

    #[test]
    fn the_top_goes_by_count_then_name_and_stops_at_the_limit() {
        let counter: HashMap<String, i64> =
            [("b", 3), ("a", 3), ("c", 9), ("d", 1)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        assert_eq!(
            top(&counter, 3),
            vec![("c".to_string(), 9), ("a".to_string(), 3), ("b".to_string(), 3)],
            "a tie goes by name"
        );
    }

    #[test]
    fn a_percent_rounds_half_to_even_like_python() {
        assert_eq!(percent(1, 8), 12, "12.5 → 12");
        assert_eq!(percent(3, 8), 38, "37.5 → 38");
        assert_eq!(percent(2, 3), 67);
        assert_eq!(percent(5, 0), 0);
    }

    #[test]
    fn heat_splits_at_the_hour_border() {
        // A full local hour (found by looking, whatever the zone of the computer is);
        // from 20 minutes past it to 20 minutes into the next one.
        let full_hour = (1_761_040_800..1_761_040_800 + 3600)
            .find(|at| formats::local_time(*at).is_some_and(|t| t.minute == 0 && t.second == 0))
            .expect("every zone has a full hour within an hour");
        let grid = heat_map(&[(full_hour + 20 * 60, full_hour + 80 * 60)]);
        let total: i64 = grid.iter().flatten().sum();
        assert_eq!(total, 3600);
        let mut cells: Vec<i64> = grid.iter().flatten().copied().filter(|n| *n > 0).collect();
        cells.sort_unstable();
        assert_eq!(cells, vec![1200, 2400], "{grid:?}");
    }

    #[test]
    fn the_tables_keep_only_the_latest_eight() {
        let many: Vec<Session> = (0..12).map(|n| session(1000 + n, |_| {})).collect();
        let latest = stability(&many, None).latest;
        assert_eq!(latest.len(), RECENT);
        assert_eq!(latest[0].start, 1011, "newest first");
        let log: Vec<Map> = (0..12)
            .map(|n| {
                vec![
                    ("name".into(), Value::str(format!("A{n}"))),
                    ("zustand".into(), Value::str(mission_log::COMPLETED)),
                    ("wann".into(), Value::str(format!("2026-10-{:02}T10:00:00", n + 1))),
                ]
            })
            .collect();
        let latest = missions(&[], &log, None).latest;
        assert_eq!(latest.len(), RECENT);
        assert_eq!(latest[0].name, "A11");
    }

    #[test]
    fn the_account_all_counts_every_session() {
        let a = session(1, |s| s.contracts = 2);
        let b = session(2, |s| {
            s.account = "Fremder".into();
            s.contracts = 5;
        });
        assert_eq!(missions(&[a.clone(), b.clone()], &[], Some("*")).completed, 7);
        assert_eq!(missions(&[a, b], &[], Some("xharig")).completed, 2);
    }

    #[test]
    fn the_overview_counts_the_own_account_and_keeps_unknown_time() {
        let mine = session(100, |s| {
            s.contracts = 4;
            s.failed = 1;
            s.jumps = 10;
            s.deaths = vec![500, 900];
            s.lost = vec!["AEGS_Sabre".into()];
        });
        let foreign = session(10_000, |s| {
            s.account = "Fremder".into();
            s.contracts = 50;
        });
        // The third period has no evaluation: its log is gone, it counts for time.
        let spans = [(100, 3700), (10_000, 13_600), (20_000, 21_000)];
        let all = summary(&[mine, foreign], &spans, None, Some("xharig"));
        assert_eq!(all.playtime, 3600 + 1000, "the foreign period is out, the unknown one counts");
        assert_eq!((all.sessions, all.recorded), (2, 1));
        assert_eq!((all.contracts, all.failed, all.jumps), (4, 1, 10));
        assert_eq!((all.deaths, all.last_death), (2, Some(900)));
        assert_eq!((all.lost, all.most_lost.as_deref()), (1, Some("AEGS_Sabre")));
        assert_eq!((all.longest, all.average, all.since), (3600, 2300, Some(100)));
    }

    #[test]
    fn a_running_session_melts_into_the_saved_one() {
        let saved = [(1000, 2000)];
        let now = summary(&[], &saved, Some((1000, 2500)), None);
        assert_eq!((now.playtime, now.sessions), (1500, 1), "not counted twice");
    }

    #[test]
    fn ships_count_once_per_session_and_weapons_per_piece() {
        let a = session(1, |s| {
            s.ships = vec!["AEGS_Sabre".into(), "DRAK_Cutlass".into()];
            s.weapons = vec![("ruecken".into(), vec![("behr_rifle".into(), 2)])];
        });
        let b = session(2, |s| {
            s.ships = vec!["AEGS_Sabre".into()];
            s.lost = vec!["AEGS_Sabre".into(), "AEGS_Sabre".into()];
            s.weapons = vec![("ruecken".into(), vec![("behr_rifle".into(), 1), ("klwe_rifle".into(), 1)])];
        });
        let c = session(3, |_| {});
        let result = ships(&[a, b, c], None);
        assert_eq!(result.ships, 2);
        assert_eq!(result.most, Some(("AEGS_Sabre".to_string(), 2)));
        assert_eq!(result.sessions_with_ship, 2, "a session without a ship is not in the base");
        assert_eq!(result.lost, vec![("AEGS_Sabre".to_string(), 2)]);
        assert_eq!(result.lost_total, 2);
        let back = &result.weapons[0];
        assert_eq!((back.slot, back.total), ("ruecken", 4));
        assert_eq!(back.top[0], ("behr_rifle".to_string(), 3));
        assert_eq!(result.weapons[1].total, 0, "the side and hand slots are there, empty");
    }

    #[test]
    fn contracts_list_names_from_the_log_and_leave_the_running_ones_out() {
        let entry = |name: &str, state: &str, when: &str, until: &str| -> Map {
            let mut m: Map = vec![
                ("name".into(), Value::str(name)),
                ("zustand".into(), Value::str(state)),
                ("wann".into(), Value::str(when)),
            ];
            if !until.is_empty() {
                m.push(("bis".into(), Value::str(until)));
            }
            m
        };
        let log = [
            entry("A", mission_log::COMPLETED, "2026-10-01T10:00:00", "2026-10-01T11:00:00"),
            entry("A", mission_log::FAILED, "2026-10-02T10:00:00", "2026-10-02T11:00:00"),
            entry("B", mission_log::RUNNING, "2026-10-03T10:00:00", ""),
            entry("C", mission_log::EXPIRED, "2026-10-04T10:00:00", ""),
        ];
        let stats = [session(1, |s| {
            s.contracts = 3;
            s.failed = 1;
            s.abandoned = 2;
        })];
        let result = missions(&stats, &log, None);
        assert_eq!((result.completed, result.failed, result.abandoned), (3, 1, 2));
        assert_eq!(result.frequent, vec![("A".to_string(), 2), ("C".to_string(), 1)]);
        let names: Vec<&str> = result.latest.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["C", "A", "A"], "newest first, the end if there is one, else the start");
        assert_eq!(result.latest[0].stamp, "2026-10-04T10:00:00");
    }

    #[test]
    fn a_crash_beats_a_clean_end_in_the_stability() {
        let clean = session(10, |s| s.clean = true);
        let crashed = session(20, |s| {
            s.clean = true;
            s.crashed = true;
            s.drops = vec![("Timeout".into(), 2), ("Nub destroyed".into(), 1)];
        });
        let open = session(30, |_| {});
        let result = stability(&[clean, crashed, open], None);
        assert_eq!((result.sessions, result.clean, result.crashed, result.open), (3, 1, 1, 1));
        assert_eq!(result.drops, 3);
        assert_eq!(result.reasons[0], ("Timeout".to_string(), 2));
        assert_eq!(result.latest[0].start, 30, "newest first");
        assert_eq!(result.latest[1].end, End::Crash);
        assert_eq!(result.latest[1].drops, 3);
        assert_eq!(reason_key("Nub destroyed"), Some("s_sb_r_nub"));
        assert_eq!(reason_key("Timeout"), None);
    }

    #[test]
    fn the_stability_names_the_joins_of_the_newest_own_session() {
        let entry: Map = match jsonio::loads(
            r#"{"von": 50, "bis": 90, "account": "Xharig",
                "shards": [[60, "pub_euw1b_12881860_420"], [null, "pub_use1a_7_8"], [70], "kaputt"]}"#,
        ) {
            Ok(Value::Object(map)) => map,
            other => panic!("{other:?}"),
        };
        let newest = Session::of(&entry);
        let shard = |id: &str| crate::shard::Shard { id: id.to_string() };
        assert_eq!(
            newest.shards,
            [
                crate::shard::Join { at: Some(60), shard: shard("pub_euw1b_12881860_420") },
                crate::shard::Join { at: None, shard: shard("pub_use1a_7_8") },
            ],
            "only pairs of a moment and a name count"
        );
        let older =
            session(10, |s| s.shards = vec![crate::shard::Join { at: Some(11), shard: shard("pub_euw1b_1_1") }]);
        let stranger = session(80, |s| s.account = "Fremder".into());
        let result = stability(&[older.clone(), newest.clone(), stranger], Some("Xharig"));
        assert_eq!(result.shards, Some((50, newest.shards.clone())), "the newest session of the own account");
        assert_eq!(stability(&[older], None).shards.map(|(from, joins)| (from, joins.len())), Some((10, 1)));
        assert_eq!(stability(&[], None).shards, None);
        // An entry of an older version has no joins.
        assert!(Session::of(&Vec::new()).shards.is_empty());
    }

    #[test]
    fn places_become_readable_by_their_pattern() {
        let names: Names =
            [("pyro6", "Terminus"), ("stanton1_l2", "HUR L2"), ("pyro2_l4", "PYR2 L4"), ("stanton2", "Crusader")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
        let read = |key: &str| {
            let p = place_parts(key, &names);
            (p.text_key, p.name)
        };
        assert_eq!(read("rs_ext_pyro6_leo"), (Some("s_sq_o_orbit"), "Terminus".to_string()));
        assert_eq!(read("rs_ext_cru-leo1"), (Some("s_sq_o_orbit"), "Crusader".to_string()));
        assert_eq!(read("rs_ext_pyro2_l4"), (Some("s_sq_o_punkt"), "PYR2 L4".to_string()));
        assert_eq!(read("RR_S1_L2"), (Some("s_sq_o_punkt"), "HUR L2".to_string()));
        assert_eq!(read("RR_S9_L2"), (Some("s_sq_o_punkt"), "RR_S9_L2".to_string()), "no name: the key");
        assert_eq!(read("rs_ext_stan-magnus_jp1"), (Some("s_sq_o_sprung"), "Stanton – Magnus".to_string()));
        assert_eq!(read("OOC_Stanton_2b_Daymar"), (None, "Daymar".to_string()));
        assert_eq!(read("levski_all-001"), (None, "Levski".to_string()));
        assert_eq!(read("NavPoint_Dynamic"), (Some("s_sq_p_wegpunkt"), String::new()));
        assert_eq!(read("MISSION_QT"), (Some("s_sq_p_auftrag"), String::new()));
        assert_eq!(read("rs_ext_nyx6_leo"), (Some("s_sq_o_orbit"), "Nyx6".to_string()), "no name: capitalised");
        assert_eq!(read("bunker_x"), (None, "bunker_x".to_string()));
    }

    #[test]
    fn a_name_comes_from_the_table_or_from_the_key() {
        let names: Names = [("vehicle_nameaegs_sabre".to_string(), "Aegis Sabre".to_string())].into_iter().collect();
        assert_eq!(display_name("AEGS_Sabre", true, &names), "Aegis Sabre");
        assert_eq!(display_name("behr_lmg_ballistic_01", false, &names), "behr lmg ballistic 01");
    }

    #[test]
    fn the_export_holds_no_account_and_tells_a_missing_value_from_zero() {
        let known = session(100, |s| {
            s.contracts = 2;
            s.present = Present { failed: false, ..Present::ALL };
            s.account = "GeheimerName".into();
        });
        // An older entry that lacks only the deaths.
        let no_deaths = session(20_000, |s| {
            s.contracts = 5;
            s.present = Present { deaths: false, ..Present::ALL };
        });
        let value = export_value(
            &[known, no_deaths],
            &[(100, 3700), (9000, 9600), (20_000, 20_600)],
            None,
            Some("GeheimerName"),
            "5.0.0",
            1_761_040_800,
        );
        let text = jsonio::dumps(&value, Some(1), false);
        assert!(!text.contains("GeheimerName"), "no player name in the file");
        let rows =
            value.get("sitzungen").and_then(|v| if let Value::Array(a) = v { Some(a) } else { None }).expect("rows");
        assert_eq!(rows[0].get("auftraege"), Some(&Value::int(2)));
        assert_eq!(rows[0].get("tode"), Some(&Value::int(0)));
        assert_eq!(rows[0].get("fehlgeschlagen"), Some(&Value::Null), "a field the entry lacks: none, not zero");
        assert_eq!(rows[1].get("auftraege"), Some(&Value::Null), "no evaluation: none, not zero");
        assert_eq!(rows[1].get("tode"), Some(&Value::Null));
        assert_eq!(rows[2].get("auftraege"), Some(&Value::int(5)));
        assert_eq!(rows[2].get("tode"), Some(&Value::Null), "the deaths are missing from the entry: none, not zero");
        assert_eq!(rows[0].get("von"), Some(&Value::str("1970-01-01T00:01:40Z")));
        assert_eq!(value.get("erstellt"), Some(&Value::str("2025-10-21T10:00:00Z")));
        assert_eq!(value.get("werkzeug"), Some(&Value::str("VerseKit")));
        assert!(value.get("gesamt").and_then(|g| g.get("waermekarte")).is_none(), "the heat map stands beside");
    }
}
