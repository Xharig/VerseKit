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

//! The lines of the log are made up the way the game writes them; the server runs on a free port of `127.0.0.1`
//! with the data folder, `APPDATA` and the game folder in a scratch folder.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

use super::*;
use crate::pypath::Flavor;

const GUID: &str = "11111111-2222-3333-4444-555555555555";
const DEFINITION: &str = "9d219d18-aeda-494c-bad6-30fb8d092949";

fn lines() -> Vec<String> {
    vec![
        format!(
            "<2026-08-09T17:43:35.422Z> [Notice] <CLocalMissionPhaseMarker::CreateMarker> Creating objective marker: \
             missionId [{GUID}], generator name [LingFamilyHauling_Hauling], contract [HaulCargo_Test_Intro], \
             contractDefinitionId[{DEFINITION}], objectiveId [pickup_1]"
        ),
        // The same contract, a second objective: no second start.
        format!(
            "<2026-08-09T17:43:35.500Z> [Notice] <CLocalMissionPhaseMarker::CreateMarker> Creating objective marker: \
             missionId [{GUID}], generator name [LingFamilyHauling_Hauling], contract [HaulCargo_Test_Intro], \
             contractDefinitionId[{DEFINITION}], objectiveId [dropoff_1]"
        ),
        format!(
            "<2026-08-09T17:50:00.000Z> [Notice] <EndMission> Ending mission for player. MissionId[{GUID}] \
             Player[Spieler] PlayerId[1] CompletionType[Complete] Reason[Mission Ended] [Team]"
        ),
        "<2026-08-09T17:50:01.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Spectre \
         (Sth/1/A): \" [138] to queue."
            .to_string(),
        "<2026-08-09T17:50:02.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Antium \
         Arme: \" [139] to queue."
            .to_string(),
    ]
}

fn pattern() -> Regex {
    Regex::new(r#"Added notification "(?:Bauplan erhalten):\s*(.+?)\s*:\s*""#).expect("pattern")
}

/// The catalog of the test: one blueprint, Spectre; a parenthesis of class and size is cut off.
fn spectre() -> EnglishName<'static> {
    Box::new(|name: &str| {
        let short = name.split(" (").next().unwrap_or(name);
        (short == "Spectre").then(|| "Spectre".to_string())
    })
}

fn reader(channel: &str) -> Reader<'static> {
    Reader::new(channel, spectre(), pattern(), Sources::default())
}

fn get<'v>(event: &'v Value, key: &str) -> &'v Value {
    event.get(key).unwrap_or(&Value::Null)
}

fn text(event: &Value, key: &str) -> String {
    get(event, key).as_str().unwrap_or_default().to_string()
}

#[test]
fn the_log_becomes_events_in_their_order_with_english_names_and_the_channel() {
    let mut read = reader("LIVE");
    let events: Vec<Value> = lines().iter().flat_map(|l| read.feed(l)).collect();
    let kinds: Vec<String> = events.iter().map(|e| text(e, "type")).collect();
    assert_eq!(kinds, ["mission_start", "mission_complete", "blueprint_received"], "one start per contract");
    let start = &events[0];
    assert_eq!(text(start, "guid"), GUID);
    assert_eq!(text(start, "debugName"), "HaulCargo_Test_Intro");
    assert_eq!(text(start, "generator"), "LingFamilyHauling_Hauling");
    assert_eq!(text(start, "contractDefinitionId"), DEFINITION);
    assert_eq!(get(start, "startTs"), &Value::Float(1_786_297_415.422));
    let end = &events[1];
    assert_eq!((text(end, "completion"), text(end, "reason")), ("Complete".into(), "Mission Ended".into()));
    let blueprint = &events[2];
    assert_eq!(text(blueprint, "productName"), "Spectre", "the English catalog name without the addition");
    assert_eq!((text(blueprint, "missionGuid"), text(blueprint, "missionTrigger")), (GUID.into(), "complete".into()));
    assert_eq!(read.unmatched, ["Antium Arme"], "an unknown name is not sent, only noted");
    assert!(events.iter().all(|e| text(e, "channel") == "LIVE"));
    // The bytes of the stream as 4.12 writes them.
    assert_eq!(
        message(start),
        format!(
            "data: {{\"type\": \"mission_start\", \"channel\": \"LIVE\", \"guid\": \"{GUID}\", \"debugName\": \
             \"HaulCargo_Test_Intro\", \"generator\": \"LingFamilyHauling_Hauling\", \"contractDefinitionId\": \
             \"{DEFINITION}\", \"startTs\": 1786297415.422}}\n\n"
        )
    );
    assert_eq!(reader("PTU").feed(&lines()[0])[0].get("channel"), Some(&Value::str("PTU")));
}

#[test]
fn the_channel_comes_from_the_name_of_the_folder() {
    assert_eq!(channel_of(Some("/x/StarCitizen/HOTFIX")), "HOTFIX");
    assert_eq!(channel_of(Some(r"C:\Spiele\StarCitizen\ptu\")), "PTU");
    assert_eq!(channel_of(Some("/x/irgendwas")), "UNKNOWN");
    assert_eq!(channel_of(None), "UNKNOWN");
}

#[test]
fn leaving_the_game_world_ends_the_open_contracts_once() {
    let mut read = reader("LIVE");
    read.feed(&lines()[0]);
    let gone = read
        .feed("<2026-08-09T18:00:00.000Z> [CSessionManager::RequestFrontEnd] Started - RequestFrontEndReason=\"x\"!");
    assert_eq!(gone.iter().map(|e| text(e, "completion")).collect::<Vec<_>>(), ["Disconnect"]);
    assert_eq!(text(&gone[0], "reason"), "Left game");
    assert!(read.snapshot().get("active").is_some_and(|a| a == &Value::Array(Vec::new())));
    let later = read.feed(&format!(
        "<2026-08-09T18:00:01.000Z> [Notice] <EndMission> Ending mission for player. MissionId[{GUID}] \
         CompletionType[Abandon] Reason[Player left]"
    ));
    assert!(later.is_empty(), "a later EndMission of the same contract: no second end");
    // An end without a start carries no contract, an end without a reason an empty one.
    let alone = reader("LIVE").feed("<2026-08-09T18:00:02.000Z> <EndMission> MissionId[abc] CompletionType[Fail]");
    assert_eq!((text(&alone[0], "type"), text(&alone[0], "reason")), ("mission_ended".into(), String::new()));
    assert_eq!(get(&alone[0], "debugName"), &Value::Null);
}

#[test]
fn a_blueprint_belongs_to_the_contract_only_within_two_minutes() {
    let mut read = reader("LIVE");
    read.feed(&lines()[0]);
    let late = read.feed(
        "<2026-08-09T17:45:36.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Spectre: \" [1]",
    );
    assert_eq!(get(&late[0], "missionGuid"), &Value::Null, "121 s after the start");
    let near = read.feed(
        "<2026-08-09T17:45:35.000Z> [Notice] <SHUDEvent_OnNotification> Added notification \"Bauplan erhalten: Spectre: \" [1]",
    );
    assert_eq!(text(&near[0], "missionTrigger"), "accept");
}

#[test]
fn a_line_without_a_stamp_gets_the_time_now_and_ports_come_from_the_new_name_first() {
    let before = now();
    let when = line_time("ohne Zeit");
    assert!(when >= before - 1.0 && when <= now() + 1.0);
    let host = |vars: &[(&str, &str)]| Host::new(Flavor::native(), vars, ".");
    assert_eq!(port(&host(&[])), PORT);
    assert_eq!(port(&host(&[(crate::legacy::ENV_SCMDB_PORT, "4000")])), 4000);
    assert_eq!(port(&host(&[(ENV_PORT, "4001"), (crate::legacy::ENV_SCMDB_PORT, "4000")])), 4001);
    assert_eq!(port(&host(&[(ENV_PORT, "kein")])), PORT);
}

#[test]
fn only_this_computer_at_the_own_port_is_a_host_that_counts() {
    assert!(host_allowed(Some("127.0.0.1:23456"), 23456));
    assert!(host_allowed(Some("LOCALHOST:23456"), 23456));
    assert!(host_allowed(Some("localhost"), 23456));
    assert!(host_allowed(Some("[::1]:23456"), 23456));
    assert!(!host_allowed(Some("127.0.0.1:1"), 23456));
    assert!(!host_allowed(Some("boese.example:23456"), 23456));
    assert!(!host_allowed(Some(""), 23456));
    assert!(!host_allowed(None, 23456));
}

/// A scratch folder with a game folder `StarCitizen/PTU` and the data folder, `APPDATA` and `LOCALAPPDATA` in it.
fn scratch(name: &str) -> (std::path::PathBuf, String, Host) {
    let root = crate::testdir::fresh(name);
    let game = root.join("StarCitizen").join("PTU");
    std::fs::create_dir_all(&game).unwrap();
    let text = |p: std::path::PathBuf| p.to_string_lossy().into_owned();
    let (home, app, local, folder) =
        (text(root.join("daten")), text(root.join("appdata")), text(root.join("local")), text(game.clone()));
    let host = Host::new(
        Flavor::native(),
        &[
            (crate::legacy::ENV_HOME, home.as_str()),
            ("APPDATA", app.as_str()),
            ("LOCALAPPDATA", local.as_str()),
            (crate::paths::ENV_INSTALL_DIR, folder.as_str()),
        ],
        &home,
    );
    (root, folder, host)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port()
}

/// One request; the status line, the heads and the body.
fn ask(port: u16, path: &str, heads: &[(&str, &str)]) -> (String, Vec<(String, String)>, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let mut request = format!("GET {path} HTTP/1.1\r\n");
    if !heads.iter().any(|(n, _)| *n == "Host") {
        request.push_str(&format!("Host: 127.0.0.1:{port}\r\n"));
    }
    for (name, value) in heads {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("Connection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).unwrap();
    let mut reader = BufReader::new(stream);
    let mut status = String::new();
    reader.read_line(&mut status).unwrap();
    let mut found = Vec::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end().to_string();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(": ").unwrap();
        if name == "Content-Length" {
            length = value.parse().unwrap();
        }
        found.push((name.to_string(), value.to_string()));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    (status.trim_end().to_string(), found, String::from_utf8(body).unwrap())
}

fn head_of<'h>(heads: &'h [(String, String)], name: &str) -> Option<&'h str> {
    heads.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// The next message of a stream that is no heartbeat.
fn next_message(reader: &mut BufReader<TcpStream>) -> String {
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if let Some(data) = line.strip_prefix("data: ") {
            let mut empty = String::new();
            reader.read_line(&mut empty).unwrap();
            return data.trim_end().to_string();
        }
    }
}

#[test]
fn the_server_answers_only_scmdb_and_this_computer_and_streams_history_state_and_news() {
    let (root, folder, host) = scratch("scmdb-bruecke");
    let log = std::path::Path::new(&folder).join("Game.log");
    std::fs::write(&log, format!("{}\n", lines()[0])).unwrap();
    let port = free_port();
    let mut vars = host.clone();
    vars.set_var(ENV_PORT, &port.to_string());
    let paths = Paths::new(vars.clone());
    assert!(!enabled(&paths), "off by default");
    assert_eq!(HEARTBEAT, Duration::from_secs(15), "protocol 0.1.9: a heartbeat every 15 s");
    let setup = |host: Host| Setup {
        host,
        version: "5.0.0-rc1+langer-zusatz".into(),
        namer: Box::new(|_| spectre()),
        poll: Duration::from_millis(100),
    };
    let mut bridge = Bridge::new();
    assert!(bridge.start(setup(vars.clone())), "the server starts on 127.0.0.1");
    std::thread::sleep(Duration::from_millis(500));
    let started = std::time::Instant::now();
    let (status, heads, body) = ask(port, "/ping", &[("Origin", "https://scmdb.net")]);
    assert!(started.elapsed() < Duration::from_millis(500), "/ping answers within 500 ms");
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(
        body,
        "{\"status\": \"ok\", \"version\": \"0.1.9\", \"channel\": \"PTU\", \"client\": \"versekit\", \"clientVersion\": \
         \"5.0.0-rc1+langer\"}"
    );
    assert_eq!(head_of(&heads, "Access-Control-Allow-Origin"), Some("https://scmdb.net"));
    assert_eq!(head_of(&heads, "Access-Control-Allow-Private-Network"), Some("true"));
    let (_, foreign, _) = ask(port, "/ping", &[("Origin", "https://boese.example")]);
    assert_eq!(head_of(&foreign, "Access-Control-Allow-Origin"), None, "a foreign page gets no CORS head");
    let (rebound, _, _) = ask(port, "/events", &[("Host", &format!("boese.example:{port}"))]);
    assert_eq!(rebound, "HTTP/1.1 403 Forbidden", "a foreign Host head is turned away though the address is 127.0.0.1");
    assert_eq!(ask(port, "/nichts", &[]).0, "HTTP/1.1 404 Not Found");

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(format!("GET /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes()).unwrap();
    let mut reader = BufReader::new(stream);
    let first = jsonio::loads(&next_message(&mut reader)).unwrap();
    assert_eq!(
        (text(&first, "type"), get(&first, "replay")),
        ("mission_start".into(), &Value::Bool(true)),
        "history first"
    );
    let snapshot = jsonio::loads(&next_message(&mut reader)).unwrap();
    assert_eq!(text(&snapshot, "type"), "state_snapshot");
    assert_eq!(snapshot.get("replay"), None);
    assert!(matches!(snapshot.get("active"), Some(Value::Array(a)) if a.len() == 1), "{snapshot:?}");
    assert_eq!(bridge.status().state, State::Connected);
    let mut file = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    writeln!(file, "{}", lines()[2]).unwrap();
    drop(file);
    let live = jsonio::loads(&next_message(&mut reader)).unwrap();
    assert_eq!((text(&live, "type"), text(&live, "channel")), ("mission_complete".into(), "PTU".into()));
    assert_eq!(live.get("replay"), None, "a new line comes live");
    // The log begins anew: a session reset, the history empty.
    std::fs::write(&log, "<2026-08-10T08:00:00.000Z> neu\n").unwrap();
    let reset = jsonio::loads(&next_message(&mut reader)).unwrap();
    assert_eq!(text(&reset, "type"), "session_reset");
    drop(reader);
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(format!("GET /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes()).unwrap();
    let mut reader = BufReader::new(stream);
    let after = jsonio::loads(&next_message(&mut reader)).unwrap();
    assert_eq!(text(&after, "type"), "state_snapshot", "after the reset no event of the old session is replayed");
    drop(reader);

    let mut second = Bridge::new();
    assert!(!second.start(setup(vars)), "the port in use: the second start does not happen");
    assert_eq!(second.status().state, State::Busy);
    bridge.stop();
    assert_eq!(bridge.status().state, State::Off);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn switched_off_and_on_again_the_history_holds_each_event_once() {
    let (root, folder, host) = scratch("scmdb-neu-an");
    let log = std::path::Path::new(&folder).join("Game.log");
    std::fs::write(&log, format!("{}\n", lines()[0])).unwrap();
    let port = free_port();
    let mut vars = host.clone();
    vars.set_var(ENV_PORT, &port.to_string());
    let setup = |host: Host| Setup {
        host,
        version: "5.0.0".into(),
        namer: Box::new(|_| spectre()),
        poll: Duration::from_millis(100),
    };
    let mut bridge = Bridge::new();
    assert!(bridge.start(setup(vars.clone())));
    std::thread::sleep(Duration::from_millis(500));
    bridge.stop();
    std::thread::sleep(Duration::from_millis(300));
    assert!(bridge.start(setup(vars)), "the port is free again after the stop");
    std::thread::sleep(Duration::from_millis(500));

    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(format!("GET /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes()).unwrap();
    let mut reader = BufReader::new(stream);
    let first = jsonio::loads(&next_message(&mut reader)).unwrap();
    assert_eq!((text(&first, "type"), get(&first, "replay")), ("mission_start".into(), &Value::Bool(true)));
    let second = jsonio::loads(&next_message(&mut reader)).unwrap();
    assert_eq!(text(&second, "type"), "state_snapshot", "the contract of the log is replayed once, not once per start");
    drop(reader);
    bridge.stop();
    let _ = std::fs::remove_dir_all(root);
}
