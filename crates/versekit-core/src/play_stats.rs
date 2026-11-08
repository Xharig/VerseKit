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

//! What happened in each session, written down for good (`statistik.json`):
//! every log is evaluated once and the result kept per session. The reading
//! half of `play_stats.py`: the evaluation of a log and the catching up of
//! the file and its read position; the sums for the pages are in
//! `stats_report`.
//!
//! ⚠ Only what stands the same in every game language is counted: contracts
//! over `<EndMission>` (only the own player, once per mission id), quantum
//! jumps, ships, weapons (each piece once per session), deaths (only the
//! number of the own body counts), crashes, lost connections. Injuries come as
//! a notification in the game language and are found by the phrases of the
//! game texts (`injuries`); zones appear only as text on screen and are not
//! counted.

use std::collections::HashSet;
use std::fs;
use std::sync::LazyLock;

use regex::Regex;

use crate::injuries::{self, Phrases, Severity};
use crate::jsonio::{self, Map, Value, map_set};
use crate::logread;
use crate::logsource::LogSource;
use crate::logtime::{Facts, FactsScan, candidate_lines, seconds, time_of};
use crate::mission_log::get;
use crate::ordered::{OrderedMap, parallel_map};
use crate::paths::Paths;
use crate::playtime::{number, write_beside};
use crate::pypath::Flavor;
use crate::text::{PY_NONSPACE, decode_replace, py_strip};

pub const FILE: &str = "statistik.json";
/// 5: the injuries of a session (`verletzungen`); every log still there is read once more.
pub const FORMAT: i64 = 5;

const QUIT: &str = "<SystemQuit>";
const QUANTUM: &str = "<Quantum Drive Arrived - Arrived at Final Destination>";
const CRASH: [&str; 2] = ["crash handler taking over", "-- GPU CRASH"];
const BODY_PORT: &str = "Body_ItemPort";
/// A corpse is one block in the same second; what comes later is a new death.
const DEATH_GAP: i64 = 10;
/// The beacon of a contract is one target for the statistics.
const MISSION_BEACON: &str = "MISSION_QT";

const DONE: &str = "Complete";
const FAILED: &str = "Fail";
const ABANDONED: &str = "Abandon";

static END_MISSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<EndMission>.*?MissionId\[([^\]]+)\].*?Player\[([^\]]*)\].*?CompletionType\[([A-Za-z]+)\]")
        .expect("fester Ausdruck")
});
static CONTROL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<Vehicle Control Flow>.*?Local client node.*?control token for '([^']+)'").expect("fester Ausdruck")
});
static DEAD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<\[ActorState\] Dead>.*?Actor '([^']*)'.*?ejected from zone '([^']+)'.*?destroyed vehicle")
        .expect("fester Ausdruck")
});
static ATTACHMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<AttachmentReceived> Player\[([^\]]*)\] Attachment\[[^,\]]*, ([^,\]]+), (\d+)\].*?Port\[([^\]]*)\]")
        .expect("fester Ausdruck")
});
static CORPSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"CSCActorCorpseUtils.*?Item '[^' ]*?_(\d+) - .*?Port Name 'Body_ItemPort'").expect("fester Ausdruck")
});
static TARGET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"<Player Selected Quantum Target - Local>.*?selected point ({PY_NONSPACE}+)"))
        .expect("fester Ausdruck")
});
static START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Projected Start Location is (.+?) for route").expect("fester Ausdruck"));
static DISCONNECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<Channel Disconnected>.*?reason="([^"]*)""#).expect("fester Ausdruck"));
/// The number at the end of an identifier: `AEGS_Sabre_465232524500` → `AEGS_Sabre`.
static SERIAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_\d{6,}$").expect("fester Ausdruck"));

/// The lines worth a look; every other line cannot change anything.
static MARKERS: LazyLock<Regex> = LazyLock::new(|| {
    let literals = [
        QUIT,
        QUANTUM,
        "<EndMission>",
        "<AttachmentReceived>",
        "CSCActorCorpseUtils",
        "<Vehicle Control Flow>",
        "<[ActorState] Dead>",
        "<Player Selected Quantum Target - Local>",
        "<Calculate Route>",
        "<Channel Disconnected>",
        crate::shard::JOIN_MARK,
        CRASH[0],
        CRASH[1],
        injuries::NOTIFICATION,
    ];
    let escaped: Vec<String> = literals.iter().map(|l| regex::escape(l)).collect();
    Regex::new(&escaped.join("|")).expect("aus maskierten Teilen gebaut")
});

/// A quantum target without its extras: `LOC_` in front and the number behind go.
///
/// ⚠ The places stand only as keys in the log. The serial number of a
/// contract beacon falls away, otherwise every beacon would be a target of its own.
pub fn place_key(raw: &str) -> String {
    let raw = raw.strip_prefix("LOC_").unwrap_or(raw);
    if raw.starts_with(MISSION_BEACON) {
        return MISSION_BEACON.to_string();
    }
    SERIAL.replace_all(raw, "").into_owned()
}

/// `wep_sidearm` → `seite` and so on; nothing for what carries no weapon.
fn weapon_slot(port: &str) -> Option<&'static str> {
    match port {
        "wep_sidearm" => Some("seite"),
        "weapon_attach_hand_right" => Some("hand"),
        _ if port.starts_with("wep_stocked_") => Some("ruecken"),
        _ => None,
    }
}

fn ints(map: OrderedMap<i64>) -> Value {
    Value::Object(map.iter().map(|(k, v)| (k.to_string(), Value::int(*v))).collect())
}

fn bump(map: &mut OrderedMap<i64>, key: &str) {
    *map.entry_or_default(key) += 1;
}

fn texts(list: Vec<String>) -> Value {
    Value::Array(list.into_iter().map(Value::Str).collect())
}

/// The session entry of one log's text, or `None` when the player never
/// arrived or the stamps make no session.
///
/// `account` is the player of this log; contracts count only when `Player[…]`
/// is the account. Without a known account everyone counts. The injuries are
/// found by the bundled phrases.
pub fn read_log_text(text: &str, account: Option<&str>) -> Option<Map> {
    let facts = Facts::of(text);
    let log = Log { spawn: facts.spawn, first: facts.first.as_deref(), last: facts.last.as_deref(), account };
    session_of(text, &log, &Phrases::table())
}

/// What a session is read with besides its lines: whether the player arrived, the first and the last stamp of the
/// whole log, and the player of the log.
struct Log<'a> {
    spawn: bool,
    first: Option<&'a str>,
    last: Option<&'a str>,
    account: Option<&'a str>,
}

/// The session entry of a log from the lines worth a look (`lines` may hold more) and the facts of the whole
/// log; `phrases` tell the injuries.
#[allow(clippy::too_many_lines)]
fn session_of(lines: &str, log: &Log<'_>, phrases: &Phrases) -> Option<Map> {
    let Log { spawn, first, last, account } = *log;
    let account = account.filter(|a| !a.is_empty());
    let own = |player: &str| account.is_none_or(|a| player.to_lowercase() == a.to_lowercase());
    let mut clean_end = false;
    let mut crashed = false;
    let mut missions: OrderedMap<String> = OrderedMap::new();
    let mut jumps: HashSet<&str> = HashSet::new();
    let mut ships: HashSet<String> = HashSet::new();
    let mut lost: Vec<String> = Vec::new();
    let mut weapons: OrderedMap<OrderedMap<HashSet<String>>> = OrderedMap::new();
    let mut targets: OrderedMap<i64> = OrderedMap::new();
    let mut selections = 0;
    let mut starts: OrderedMap<i64> = OrderedMap::new();
    let mut start_pending = false;
    let mut disconnects: OrderedMap<i64> = OrderedMap::new();
    let mut bodies: HashSet<String> = HashSet::new();
    let mut deaths: Vec<i64> = Vec::new();
    let mut shards: Vec<crate::shard::Join> = Vec::new();
    let mut hurt = injuries::Count::default();

    for line in candidate_lines(lines, &MARKERS) {
        let stamp = Some(time_of(line)).filter(|s| !s.is_empty());
        // ⚠ The order of these branches is the order of Python's `elif` chain: a
        // line with a quantum mark but no stamp falls through to the next ones.
        if line.contains(QUIT) {
            clean_end = true;
        } else if line.contains(QUANTUM) && stamp.is_some() {
            jumps.extend(stamp);
        } else if line.contains("<EndMission>") {
            if let Some(c) = END_MISSION.captures(line)
                && own(&c[2])
            {
                missions.set(&c[1], c[3].to_string());
            }
        } else if line.contains("<AttachmentReceived>") {
            if let Some(c) = ATTACHMENT.captures(line) {
                if own(&c[1]) && &c[4] == BODY_PORT {
                    bodies.insert(c[3].to_string());
                // ⚠ `Default` is an empty placeholder of the game, no weapon.
                } else if own(&c[1])
                    && &c[2] != "Default"
                    && let Some(slot) = weapon_slot(&c[4])
                {
                    weapons.entry_or_default(slot).entry_or_default(&c[2]).insert(c[3].to_string());
                }
            }
        } else if line.contains("CSCActorCorpseUtils") {
            let moment = seconds(stamp.unwrap_or("")).filter(|m| *m != 0);
            if let Some(c) = CORPSE.captures(line)
                && bodies.contains(&c[1])
                && let Some(moment) = moment
                && deaths.last().is_none_or(|last| moment - last > DEATH_GAP)
            {
                deaths.push(moment);
            }
        } else if line.contains("<Vehicle Control Flow>") {
            if let Some(c) = CONTROL.captures(line) {
                ships.insert(SERIAL.replace_all(&c[1], "").into_owned());
            }
        } else if line.contains("<[ActorState] Dead>") {
            if let Some(c) = DEAD.captures(line)
                && own(&c[1])
            {
                lost.push(SERIAL.replace_all(&c[2], "").into_owned());
            }
        } else if line.contains("<Player Selected Quantum Target - Local>") {
            if let Some(c) = TARGET.captures(line) {
                selections += 1;
                start_pending = true;
                bump(&mut targets, &place_key(&c[1]));
            }
        } else if start_pending && line.contains("<Calculate Route>") {
            // ⚠ Only the first route calculation after choosing a target: the
            // game often calculates the same route several times.
            if let Some(c) = START.captures(line) {
                start_pending = false;
                bump(&mut starts, py_strip(&c[1]));
            }
        } else if line.contains("<Channel Disconnected>") {
            if let Some(c) = DISCONNECT.captures(line) {
                bump(&mut disconnects, py_strip(&c[1]));
            }
        } else if line.contains(crate::shard::JOIN_MARK) {
            shards.extend(crate::shard::join_in(line));
        } else if line.contains(injuries::NOTIFICATION) {
            if let Some((severity, text)) = phrases.notice(line) {
                hurt.add(severity, text, seconds(stamp.unwrap_or("")).filter(|m| *m != 0));
            }
        } else if !crashed && CRASH.iter().any(|mark| line.contains(mark)) {
            crashed = true;
        }
    }
    if !spawn {
        return None;
    }
    let start = seconds(first.unwrap_or("")).filter(|s| *s != 0)?;
    let end = seconds(last.unwrap_or("")).filter(|s| *s != 0)?;
    if end < start {
        return None;
    }
    let count = |kind: &str| missions.values().filter(|k| k.as_str() == kind).count() as i64;
    let mut ship_list: Vec<String> = ships.into_iter().collect();
    ship_list.sort();
    let weapons = Value::Object(
        weapons
            .iter()
            .map(|(slot, found)| {
                let classes =
                    found.iter().map(|(class, ids)| (class.to_string(), Value::int(ids.len() as i64))).collect();
                (slot.to_string(), Value::Object(classes))
            })
            .collect(),
    );
    Some(vec![
        ("von".into(), Value::int(start)),
        ("bis".into(), Value::int(end)),
        ("sauber".into(), Value::Bool(clean_end)),
        ("account".into(), Value::str(account.unwrap_or(""))),
        ("auftraege".into(), Value::int(count(DONE))),
        ("fehlgeschlagen".into(), Value::int(count(FAILED))),
        ("abgebrochen".into(), Value::int(count(ABANDONED))),
        ("spruenge".into(), Value::int(jumps.len() as i64)),
        ("schiffe".into(), texts(ship_list)),
        ("verloren".into(), texts(lost)),
        ("tode".into(), Value::Array(deaths.into_iter().map(Value::int).collect())),
        ("waffen".into(), weapons),
        ("zielwahlen".into(), Value::int(selections)),
        ("ziele".into(), ints(targets)),
        ("starts".into(), ints(starts)),
        ("absturz".into(), Value::Bool(crashed)),
        ("abbrueche".into(), ints(disconnects)),
        ("shards".into(), shard_list(shards)),
        ("verletzungen".into(), injury_counts(hurt.counts())),
    ])
}

/// The injuries of a session as `{"leicht": n, "mittel": n, "schwer": n}`.
fn injury_counts(counts: [i64; 3]) -> Value {
    Value::Object(Severity::ALL.iter().map(|s| (s.stored().to_string(), Value::int(counts[s.index()]))).collect())
}

/// The joins of a session as `[[moment, shard], …]`; a join without a stamp has `null` as its moment. Only the name
/// of the shard, never the address of the server.
fn shard_list(joins: Vec<crate::shard::Join>) -> Value {
    Value::Array(
        joins
            .into_iter()
            .map(|join| Value::Array(vec![join.at.map_or(Value::Null, Value::int), Value::Str(join.shard.id)]))
            .collect(),
    )
}

/// What [`read_log`] keeps of a log it read: the lines worth a look so far, each ended by `\n`, and the facts of
/// the whole log.
#[derive(Default)]
struct Seen {
    lines: String,
    facts: FactsScan,
}

impl Seen {
    /// Takes the next piece of the log: whole lines.
    fn piece(&mut self, text: &str) {
        self.facts.piece(text);
        for line in candidate_lines(text, &MARKERS) {
            self.lines.push_str(line);
            self.lines.push('\n');
        }
    }

    fn entry(&self, account: Option<&str>, phrases: &Phrases) -> Option<Map> {
        let facts = self.facts.facts();
        let log = Log { spawn: facts.spawn, first: facts.first.as_deref(), last: facts.last.as_deref(), account };
        session_of(&self.lines, &log, phrases)
    }
}

/// [`read_log_text`] of a log file; an unreadable one has none.
///
/// ⚠ The file is read a piece at a time and only the lines worth a look are kept, so a log of gigabytes never
/// stands in memory whole. A line longer than one piece is read in parts.
pub fn read_log(path: &str, account: Option<&str>) -> Option<Map> {
    read_log_with(path, account, &Phrases::table())
}

/// [`read_log`] with the phrases of the injuries of the installation.
pub fn read_log_with(path: &str, account: Option<&str>, phrases: &Phrases) -> Option<Map> {
    let mut seen = Seen::default();
    logread::read_all(path, |bytes| seen.piece(&decode_replace(bytes))).ok()?;
    seen.entry(account, phrases)
}

/// `{'format':…, 'sitzungen': {von: {…}}, 'gelesen': {name: size}}`.
///
/// ⚠ An older format keeps its sessions but loses the read position: so every
/// log still there is read once with the new counts, and what is already gone
/// keeps its old numbers.
pub fn load(paths: &Paths) -> Map {
    let valid = |d: &Value| matches!(d, Value::Object(_));
    match jsonio::load_json(&paths.app_file(FILE), Some(&valid), "play_stats.load") {
        Some(Value::Object(mut map)) if matches!(get(&map, "sitzungen"), Some(Value::Object(_))) => {
            if number(get(&map, "format")) != Some(FORMAT as f64) {
                map_set(&mut map, "gelesen", Value::Object(Vec::new()));
            }
            if get(&map, "gelesen").is_none() {
                map.push(("gelesen".into(), Value::Object(Vec::new())));
            }
            map
        }
        _ => vec![
            ("format".into(), Value::int(FORMAT)),
            ("sitzungen".into(), Value::Object(Vec::new())),
            ("gelesen".into(), Value::Object(Vec::new())),
        ],
    }
}

/// Writes first beside, then renames; reports a failure.
pub fn save(paths: &Paths, data: &mut Map) -> bool {
    write_beside(paths, FILE, data, FORMAT, "play_stats.save")
}

/// Evaluates the logs and writes `statistik.json` on; returns the number of
/// newly recorded sessions.
///
/// ⚠ Its own read position (name and size), like the playtime. ⚠ The key is
/// the start of the session, not the file name: the running `Game.log` is
/// renamed at the next start and would count a second time.
///
/// `clock` is the time written as `stand`.
pub fn catch_up(src: &LogSource<'_>, files: &[String], clock: &dyn Fn() -> f64) -> usize {
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
    // The phrases of the injuries in the languages of the installation, read once for all logs.
    let phrases = if jobs.is_empty() {
        Phrases::default()
    } else {
        Phrases::of(&crate::logsource::LogSource::sources_of(paths))
    };
    let entries: Vec<Map> = parallel_map(&jobs, |(path, account)| read_log_with(path, account.as_deref(), &phrases))
        .into_iter()
        .flatten()
        .collect();
    // ⚠ The clock is read once per run, whether or not anything is written: its calls are counted.
    let now = clock();
    if marks.is_empty() {
        return 0;
    }
    // ⚠ The reading above takes long and runs without the lock; what is
    // written is loaded anew under it, or two writers (this page and the
    // reading of the contract log) would overwrite each other's sessions.
    let _guard = jsonio::lock_saves();
    let mut data = load(paths);
    let mut sessions: Map = match get(&data, "sitzungen") {
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
    for entry in entries {
        let key = get(&entry, "von").map_or_else(String::new, |v| jsonio::dumps(v, None, false));
        if get(&sessions, &key).is_none() {
            added += 1;
        }
        map_set(&mut sessions, &key, Value::Object(entry));
    }
    map_set(&mut data, "sitzungen", Value::Object(sessions));
    map_set(&mut data, "gelesen", Value::Object(read));
    map_set(&mut data, "stand", Value::Float(now));
    save(paths, &mut data);
    added
}

/// Every log of the installation: the backups and the running one
/// (`log_files`).
pub fn log_files(paths: &Paths) -> Vec<String> {
    let mut files = paths.log_backups(None);
    if let Some(game) = paths.game_folder() {
        let running = Flavor::native().join(&game, "Game.log");
        if std::path::Path::new(&running).is_file() {
            files.push(running);
        }
    }
    files
}

/// "Evaluate everything anew": every log there is is read once more.
///
/// ⚠ Only the **read position** is dropped, not the sessions. What has long
/// since vanished from the logs would otherwise be gone for good after one
/// click, which is what this file exists to prevent.
pub fn rescan(src: &LogSource<'_>, files: &[String], clock: &dyn Fn() -> f64) -> usize {
    {
        let _guard = jsonio::lock_saves();
        let mut data = load(src.paths);
        map_set(&mut data, "gelesen", Value::Object(Vec::new()));
        save(src.paths, &mut data);
    }
    catch_up(src, files, clock)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(lines: &[&str]) -> String {
        let mut out = String::from("<2026-10-01T10:00:00.000Z> Start OnClientSpawned\n");
        for line in lines {
            out.push_str("<2026-10-01T10:00:05.000Z> ");
            out.push_str(line);
            out.push('\n');
        }
        out.push_str("<2026-10-01T10:30:00.000Z> Ende\n");
        out
    }

    fn entry(lines: &[&str], account: Option<&str>) -> Map {
        read_log_text(&log(lines), account).expect("Sitzung")
    }

    fn int(map: &Map, key: &str) -> i64 {
        match get(map, key) {
            Some(Value::Int(t)) => t.parse().expect("Zahl"),
            other => panic!("{key}: {other:?}"),
        }
    }

    /// The injuries of a session: each severity on its own, a repeat within ten seconds once, no injury and other
    /// notifications not at all; read the same from the file a piece at a time.
    #[test]
    fn the_injuries_of_a_session_count_by_their_severity() {
        let note = |stamp: &str, text: &str| {
            format!("<2026-10-01T10:{stamp}.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{text}: \" [3]\n")
        };
        let mut text = String::from("<2026-10-01T10:00:00.000Z> Start OnClientSpawned\n");
        for (stamp, phrase) in [
            ("01:00", "Minor Injury Detected"),
            ("01:04", "Minor Injury Detected"),
            ("05:00", "Leichte Verletzung erkannt"),
            ("06:00", "Moderate Injury Detected"),
            ("07:00", "Severe Injury Detected"),
            ("08:00", "No Injury Detected"),
            ("09:00", "Bauplan erhalten: Helm"),
        ] {
            text.push_str(&note(stamp, phrase));
        }
        text.push_str("<2026-10-01T10:30:00.000Z> Ende\n");
        let entry = read_log_text(&text, None).expect("Sitzung");
        let Some(Value::Object(hurt)) = get(&entry, "verletzungen") else { panic!("verletzungen") };
        assert_eq!((int(hurt, "leicht"), int(hurt, "mittel"), int(hurt, "schwer")), (2, 1, 1));
        let dir = crate::testdir::fresh("statistik-verletzungen");
        let file = dir.join("Game.log");
        fs::write(&file, &text).unwrap();
        assert_eq!(read_log(&file.to_string_lossy(), None), Some(entry), "the same a piece at a time");
        // A session without any: all three zero.
        let calm = entry_of(&[]);
        let Some(Value::Object(none)) = get(&calm, "verletzungen") else { panic!("verletzungen") };
        assert_eq!((int(none, "leicht"), int(none, "mittel"), int(none, "schwer")), (0, 0, 0));
    }

    /// The shape of a game log of 04.11.2026: one injury is one `Added notification` line, followed by a line with
    /// only its text and the lines of `<UpdateNotificationItem>`; it counts once.
    #[test]
    fn one_injury_in_the_shape_of_the_game_log_counts_once() {
        let text = "Leichte Verletzung erkannt - Linker Arm - Behandlung der Stufe 3 erforderlich : ";
        let log = format!(
            "<2026-11-04T17:40:00.000Z> Start OnClientSpawned\n\
             <2026-11-04T17:45:49.599Z> [Notice] <SHUDEvent_OnNotification> Added notification \"{text}\" [91] to queue. \
             New queue size: 1, MissionId: [00000000-0000-0000-0000-000000000000], ObjectiveId: [] \
             [Team_CoreGameplayFeatures][Missions][Comms]\n\
             \"{text}\" [91]\n\
             <2026-11-04T17:45:49.600Z> [Notice] <UpdateNotificationItem> Notification \"{text}\" [91], Action: Next\n\
             <2026-11-04T17:45:54.600Z> [Notice] <UpdateNotificationItem> Notification \"{text}\" [91], Action: StartFade\n\
             <2026-11-04T17:45:55.600Z> [Notice] <UpdateNotificationItem> Notification \"{text}\" [91], Action: Remove\n\
             <2026-11-04T18:00:00.000Z> Ende\n"
        );
        let entry = read_log_text(&log, None).expect("Sitzung");
        let Some(Value::Object(hurt)) = get(&entry, "verletzungen") else { panic!("verletzungen") };
        assert_eq!((int(hurt, "leicht"), int(hurt, "mittel"), int(hurt, "schwer")), (1, 0, 0));
    }

    fn entry_of(lines: &[&str]) -> Map {
        entry(lines, None)
    }

    #[test]
    fn contracts_count_per_mission_id_for_the_own_player_only() {
        let lines = [
            "<EndMission> a MissionId[m1] Player[Xharig] CompletionType[Complete]",
            "<EndMission> a MissionId[m1] Player[Xharig] CompletionType[Complete]",
            "<EndMission> a MissionId[m2] Player[xharig] CompletionType[Fail]",
            "<EndMission> a MissionId[m3] Player[Xharig] CompletionType[Abandon]",
            "<EndMission> a MissionId[m4] Player[Fremder] CompletionType[Complete]",
            "<EndMission> a MissionId[m5] Player[Xharig] CompletionType[Deactivate]",
        ];
        let e = entry(&lines, Some("XHARIG"));
        assert_eq!((int(&e, "auftraege"), int(&e, "fehlgeschlagen"), int(&e, "abgebrochen")), (1, 1, 1));
        // Without a known account everyone counts.
        let all = entry(&lines, None);
        assert_eq!(int(&all, "auftraege"), 2);
        assert_eq!(get(&all, "account"), Some(&Value::str("")));
    }

    #[test]
    fn a_dead_actor_is_no_death_only_the_own_corpse_is() {
        let own_body =
            "<AttachmentReceived> Player[Xharig] Attachment[x, body_01_a, 4711] Status[y] Port[Body_ItemPort]";
        let corpse = |n: &str| {
            format!(
                "CSCActorCorpseUtils::PopulateItemPortForItemRecoveryEntitlement Item 'body_01_a_{n} - x' Port Name 'Body_ItemPort'"
            )
        };
        let own = corpse("4711");
        let foreign = corpse("9999");
        let dead = "<[ActorState] Dead> Actor 'Xharig' ejected from zone 'AEGS_Sabre_465232524500' destroyed vehicle";
        let e = entry(&[own_body, &own, &own, &foreign, dead], Some("Xharig"));
        // Two lines in the same second are one corpse; a foreign body does not count.
        assert_eq!(
            get(&e, "tode"),
            Some(&Value::Array(vec![Value::int(seconds("2026-10-01T10:00:05").expect("Zeit"))]))
        );
        assert_eq!(get(&e, "verloren"), Some(&Value::Array(vec![Value::str("AEGS_Sabre")])));
    }

    #[test]
    fn a_new_death_needs_more_than_ten_seconds() {
        let body = "<AttachmentReceived> Player[Xharig] Attachment[x, body_01_a, 1] Status[y] Port[Body_ItemPort]";
        let corpse = "CSCActorCorpseUtils Item 'b_1 - x' Port Name 'Body_ItemPort'";
        let text = format!(
            "<2026-10-01T10:00:00.000Z> OnClientSpawned\n<2026-10-01T10:00:01.000Z> {body}\n\
             <2026-10-01T10:00:05.000Z> {corpse}\n<2026-10-01T10:00:15.000Z> {corpse}\n<2026-10-01T10:00:16.000Z> {corpse}\n\
             <2026-10-01T10:01:00.000Z> {corpse}\n"
        );
        let e = read_log_text(&text, Some("Xharig")).expect("Sitzung");
        let Some(Value::Array(deaths)) = get(&e, "tode") else { panic!("tode") };
        // 10 s later is still the same block, 11 s later a new death.
        assert_eq!(deaths.len(), 3);
    }

    #[test]
    fn weapons_once_per_piece_and_no_default() {
        let weapon = |class: &str, id: &str, port: &str| {
            format!("<AttachmentReceived> Player[Xharig] Attachment[x, {class}, {id}] Status[y] Port[{port}]")
        };
        let a = weapon("klwe_pistol", "1", "wep_sidearm");
        let b = weapon("klwe_pistol", "1", "wep_sidearm");
        let c = weapon("behr_rifle", "2", "wep_stocked_2");
        let d = weapon("Default", "3", "wep_stocked_3");
        let e = weapon("behr_rifle", "4", "wep_stocked_3");
        let f = weapon("klwe_pistol", "5", "weapon_attach_hand_right");
        let g = weapon("klwe_pistol", "6", "unknown_port");
        let result = entry(&[&a, &b, &c, &d, &e, &f, &g], Some("Xharig"));
        let text = jsonio::dumps(get(&result, "waffen").expect("waffen"), None, false);
        assert_eq!(text, r#"{"seite": {"klwe_pistol": 1}, "ruecken": {"behr_rifle": 2}, "hand": {"klwe_pistol": 1}}"#);
    }

    #[test]
    fn targets_starts_ships_disconnects_and_crashes() {
        let lines = [
            "<Player Selected Quantum Target - Local> x selected point LOC_rs_ext_pyro6_leo_12345678",
            "<Calculate Route> Projected Start Location is  Terminus  for route",
            "<Calculate Route> Projected Start Location is Anders for route",
            "<Player Selected Quantum Target - Local> x selected point MISSION_QT_Quantum_Beacon_TSG_1",
            "<Quantum Drive Arrived - Arrived at Final Destination>",
            "<Vehicle Control Flow> a Local client node b control token for 'AEGS_Sabre_465232524500'",
            "<Channel Disconnected> x reason=\"Timeout\"",
            "<Channel Disconnected> x reason=\"Timeout\"",
            "crash handler taking over",
            "<SystemQuit>",
        ];
        let e = entry(&lines, None);
        assert_eq!(
            jsonio::dumps(get(&e, "ziele").expect("ziele"), None, false),
            r#"{"rs_ext_pyro6_leo": 1, "MISSION_QT": 1}"#
        );
        assert_eq!(jsonio::dumps(get(&e, "starts").expect("starts"), None, false), r#"{"Terminus": 1}"#);
        assert_eq!(int(&e, "zielwahlen"), 2);
        assert_eq!(int(&e, "spruenge"), 1);
        assert_eq!(get(&e, "schiffe"), Some(&Value::Array(vec![Value::str("AEGS_Sabre")])));
        assert_eq!(jsonio::dumps(get(&e, "abbrueche").expect("abbrueche"), None, false), r#"{"Timeout": 2}"#);
        assert_eq!(get(&e, "absturz"), Some(&Value::Bool(true)));
        assert_eq!(get(&e, "sauber"), Some(&Value::Bool(true)));
    }

    #[test]
    fn the_joins_of_the_session_keep_the_shard_and_its_moment_but_not_the_address() {
        let join = |shard: &str| {
            format!("[Notice] <Join PU> address[35.241.242.86] port[64388] shard[{shard}] locationId[5629]")
        };
        let e = entry(&[&join("pub_euw1b_12881860_420"), "nothing", &join("pub_euw1b_12881999_17")], None);
        let moment = seconds("2026-10-01T10:00:05").expect("Zeit");
        assert_eq!(
            jsonio::dumps(get(&e, "shards").expect("shards"), None, false),
            format!(r#"[[{moment}, "pub_euw1b_12881860_420"], [{moment}, "pub_euw1b_12881999_17"]]"#)
        );
        let whole = jsonio::dumps(&Value::Object(e), None, false);
        for part in ["35.241.242.86", "64388", "5629"] {
            assert!(!whole.contains(part), "{part} must not be kept: {whole}");
        }
        // A session without a join has an empty list.
        assert_eq!(get(&entry(&["nothing"], None), "shards"), Some(&Value::Array(Vec::new())));
    }

    #[test]
    fn a_quantum_line_without_a_stamp_falls_through_the_chain() {
        // No stamp on this line (two lines joined by a lone carriage return would
        // give one): the quantum branch is skipped, the next one still runs.
        let text = "<2026-10-01T10:00:00.000Z> OnClientSpawned\nohne Stempel <Quantum Drive Arrived - Arrived at Final Destination> <EndMission> a MissionId[m1] Player[X] CompletionType[Complete]\n<2026-10-01T10:30:00.000Z> Ende\n";
        let e = read_log_text(text, None).expect("Sitzung");
        assert_eq!((int(&e, "spruenge"), int(&e, "auftraege")), (0, 1));
    }

    /// The statistics page and the reading of the contract log write the same
    /// file at the same time: no session may be lost and the file stays whole.
    #[test]
    fn two_writers_at_once_lose_no_session() {
        use crate::legacy;
        use crate::paths::Host;

        let root = std::env::temp_dir().join(format!("versekit-statistik-zwei-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let (home, logs) = (root.join("home"), root.join("logs"));
        fs::create_dir_all(&home).expect("Ordner");
        fs::create_dir_all(&logs).expect("Ordner");
        let files: Vec<String> = (0..24)
            .map(|n| {
                let file = logs.join(format!("{n:02}.log"));
                let day = n + 1;
                let text = format!(
                    "<2026-09-{day:02}T10:00:00.000Z> Start OnClientSpawned\n<2026-09-{day:02}T11:00:00.000Z> <SystemQuit>\n"
                );
                fs::write(&file, text).expect("Datei");
                file.to_str().expect("Pfad").to_string()
            })
            .collect();
        let host = Host::new(
            Flavor::native(),
            &[(legacy::ENV_HOME, home.to_str().expect("Pfad"))],
            root.to_str().expect("Pfad"),
        );
        let paths = Paths::new(host);
        std::thread::scope(|scope| {
            for part in 0..2 {
                let (paths, files) = (&paths, &files);
                scope.spawn(move || {
                    for round in 0..6 {
                        let src = LogSource::new(paths, LogSource::sources_of(paths));
                        // Each writer sees the logs in its own order and size state.
                        let mine: Vec<String> = files.iter().skip((round + part) % 3).step_by(1).cloned().collect();
                        catch_up(&src, &mine, &|| 0.0);
                        rescan(&src, &mine[..mine.len() / 2], &|| 0.0);
                    }
                });
            }
        });
        let src = LogSource::new(&paths, LogSource::sources_of(&paths));
        catch_up(&src, &files, &|| 0.0);
        let data = load(&paths);
        let Some(Value::Object(sessions)) = get(&data, "sitzungen") else { panic!("Sitzungen") };
        assert_eq!(sessions.len(), 24, "every session is there");
        let broken = fs::read_dir(&home)
            .expect("Ordner")
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains("kaputt"));
        assert!(!broken, "no file set aside as broken");
        let _ = fs::remove_dir_all(&root);
    }

    /// "Evaluate all anew" drops the read position, not the sessions: a log that
    /// is read again brings its session back, one that is gone keeps its own.
    #[test]
    fn a_rescan_reads_every_log_again_and_keeps_the_sessions() {
        use crate::legacy;
        use crate::paths::Host;

        let root = std::env::temp_dir().join(format!("versekit-statistik-neu-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let (home, logs) = (root.join("home"), root.join("logs"));
        fs::create_dir_all(&home).expect("Ordner");
        fs::create_dir_all(&logs).expect("Ordner");
        let log = logs.join("a.log");
        fs::write(&log, "<2026-09-01T10:00:00.000Z> Start OnClientSpawned\n<2026-09-01T11:00:00.000Z> <SystemQuit>\n")
            .expect("Datei");
        let files = vec![log.to_str().expect("Pfad").to_string()];
        let host = Host::new(
            Flavor::native(),
            &[(legacy::ENV_HOME, home.to_str().expect("Pfad"))],
            root.to_str().expect("Pfad"),
        );
        let paths = Paths::new(host);
        let src = LogSource::new(&paths, LogSource::sources_of(&paths));
        assert_eq!(catch_up(&src, &files, &|| 0.0), 1);
        // The log is known by name and size: nothing is read.
        let reads = std::cell::Cell::new(0);
        assert_eq!(
            catch_up(&src, &files, &|| {
                reads.set(reads.get() + 1);
                0.0
            }),
            0
        );
        assert_eq!(reads.get(), 1, "the clock is read once, though nothing is written");
        // A session of a log that is long gone, and the session of this log taken out by hand.
        let mut data = load(&paths);
        let Some(Value::Object(sessions)) = get(&data, "sitzungen").cloned() else { panic!("Sitzungen") };
        let mut old = sessions[0].1.as_object().expect("Eintrag").clone();
        map_set(&mut old, "von", Value::int(5));
        map_set(&mut data, "sitzungen", Value::Object(vec![("5".into(), Value::Object(old))]));
        save(&paths, &mut data);
        // Still known by name and size: a plain run brings nothing back.
        assert_eq!(catch_up(&src, &files, &|| 0.0), 0);
        let count = |paths: &Paths| match get(&load(paths), "sitzungen") {
            Some(Value::Object(map)) => map.len(),
            _ => 0,
        };
        assert_eq!(count(&paths), 1, "only the old one");
        // The rescan reads the log again; the old session stays.
        assert_eq!(rescan(&src, &files, &|| 0.0), 1);
        assert_eq!(count(&paths), 2, "the old one stays, the log's own is back");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn no_session_without_arrival_or_stamps() {
        assert_eq!(read_log_text("<2026-10-01T10:00:00.000Z> nichts\n", None), None);
        assert_eq!(read_log_text("OnClientSpawned ohne Stempel", None), None);
    }

    #[test]
    fn place_key_strips_prefix_serial_and_beacons() {
        assert_eq!(place_key("LOC_rs_ext_pyro6_leo"), "rs_ext_pyro6_leo");
        assert_eq!(place_key("Beacon_123456"), "Beacon");
        assert_eq!(place_key("Beacon_12345"), "Beacon_12345");
        assert_eq!(place_key("LOC_MISSION_QT_Quantum_Beacon_TSG_1234567"), "MISSION_QT");
    }
}
