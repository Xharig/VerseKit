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

//! The lines of these logs are made up; none of them is from a real log.

use super::*;
use crate::envvars;
use crate::paths::Host;

/// The name of the own ship in the lines of a log (with the serial number the game gives it).
const OWN: &str = "RSI_Constellation_Andromeda_GS_465232524500";
const FOREIGN: &str = "RSI_Polaris_777777777777";

/// The stamp of the second `n` after 10:00:00 on 2026-10-01.
fn stamp(n: i64) -> String {
    let s = 10 * 3600 + n;
    format!("<2026-10-01T{:02}:{:02}:{:02}.000Z>", s / 3600, s % 3600 / 60, s % 60)
}

/// A made-up log: the player arrives at second 0, the log ends at second 1800.
fn log(lines: &[(i64, &str)]) -> String {
    let mut out = format!("{} Start OnClientSpawned\n", stamp(0));
    for (at, text) in lines {
        out.push_str(&format!("{} {text}\n", stamp(*at)));
    }
    out.push_str(&format!("{} Ende\n", stamp(1800)));
    out
}

fn session_of(lines: &[(i64, &str)]) -> Session {
    read_log_text(&log(lines), Some("Xharig"), &Phrases::table()).expect("a session")
}

fn of(session: &Session, kind: Kind) -> Vec<&Event> {
    session.events.iter().filter(|e| e.kind == kind).collect()
}

fn target(ship: &str, point: &str) -> String {
    format!("<Player Selected Quantum Target - Local> a | {ship}[900001]|b Player has selected point {point} as their destination")
}

fn arrival(ship: &str) -> String {
    format!("<Quantum Drive Arrived - Arrived at Final Destination> a | {ship}[900002]|b")
}

fn change(before: &str, now: &str) -> String {
    format!("<Update Inventory Location> Player [Xharig] is changing location. Landing [{before}] -> [{now}]. Location [0] -> [5]")
}

fn attachment(player: &str, number: &str) -> String {
    format!("<AttachmentReceived> Player[{player}] Attachment[a, body_01_noMagicPocket, {number}] x Port[Body_ItemPort]")
}

fn corpse(number: &str) -> String {
    format!("<Adding non kept item [CSCActorCorpseUtils::x]> Item 'body_01_noMagicPocket_{number} - 12' x Port Name 'Body_ItemPort'")
}

fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("versekit-flugbuch-{name}-{}", crate::testdir::run_tag()));
    let _ = fs::remove_dir_all(&root);
    let home = root.join("home");
    fs::create_dir_all(&home).expect("folder");
    let host = Host::new(Flavor::native(), &[(envvars::ENV_HOME, home.to_str().expect("path"))], root.to_str().expect("path"));
    (Paths::new(host), root)
}

#[test]
fn a_log_without_spawn_has_no_session() {
    let text = format!("{} <SystemQuit>\n{} Ende\n", stamp(5), stamp(1800));
    assert_eq!(read_log_text(&text, None, &Phrases::table()), None);
    let with = read_log_text(&log(&[(5, "<SystemQuit>")]), None, &Phrases::table()).expect("a session");
    assert_eq!((with.start, with.end), (with.start, with.start + 1800));
    assert_eq!(with.count(Kind::Start), 1, "the arrival is the start");
}

#[test]
fn a_landing_names_the_place_and_zero_means_underway() {
    let a = change("0", "123");
    let b = change("123", "0");
    let c = change("0", "0");
    let d = change("7", "7");
    let session = session_of(&[(10, &a), (20, &b), (30, &c), (40, &d)]);
    let places: Vec<(i64, &str)> =
        of(&session, Kind::Place).iter().map(|e| (e.at - session.start, e.data.place.as_deref().unwrap_or(""))).collect();
    assert_eq!(places, [(10, "123"), (20, "0")], "[0] to [0] and [a] to [a] change nothing");
}

#[test]
fn only_the_own_ship_arrives() {
    let before = arrival(OWN);
    let pick = target(OWN, "LOC_rs_ext_nyx-castra_jp1");
    let own = arrival(OWN);
    let foreign = arrival(FOREIGN);
    let session = session_of(&[(5, &before), (10, &pick), (20, &foreign), (30, &own)]);
    let jumps = of(&session, Kind::Jump);
    assert_eq!(jumps.len(), 1, "the ship nearby and the arrival before any choice are not ours");
    assert_eq!(jumps[0].at - session.start, 30);
    assert_eq!(jumps[0].data.ship.as_deref(), Some("RSI_Constellation_Andromeda_GS"), "the class has no serial number");
    assert_eq!(jumps[0].data.target.as_deref(), Some("rs_ext_nyx-castra_jp1"));
}

#[test]
fn a_ship_the_seat_was_left_in_is_the_own_one() {
    let left = format!("<Vehicle Control Flow> ClearDriver: Local client node [5] releasing control token for '{FOREIGN}' [9]");
    let own = arrival(FOREIGN);
    let session = session_of(&[(10, &left), (20, &own)]);
    assert_eq!(of(&session, Kind::Ship).len(), 1);
    assert_eq!(of(&session, Kind::Jump).len(), 1, "a ship the player left the seat of is theirs");
}

#[test]
fn the_last_ship_is_the_last_seat_or_target_and_a_ship_only_passing_by_is_none() {
    let seat = format!("<Vehicle Control Flow> CreateDriver: Local client node [5] granted control token for '{FOREIGN}' [9]");
    let pick = target(OWN, "LOC_rs_ext_nyx-castra_jp1");
    let passing = arrival("MISC_Prospector_111111111111");
    assert_eq!(last_ship_in(&log(&[(10, &seat)])).as_deref(), Some("RSI_Polaris"));
    assert_eq!(last_ship_in(&log(&[(10, &seat), (20, &pick)])).as_deref(), Some("RSI_Constellation_Andromeda_GS"));
    assert_eq!(last_ship_in(&log(&[(10, &pick), (20, &seat), (30, &passing)])).as_deref(), Some("RSI_Polaris"));
    assert_eq!(last_ship_in(&log(&[(30, &passing)])), None, "an arrival alone names no ship of the player");
    assert_eq!(last_ship_in(""), None);
}

#[test]
fn a_target_number_is_a_planet_when_the_log_says_so() {
    let planet = "<Planet CPU Maps Loaded> x Planet 'nyx1' (entity 4242) y";
    let known = target(OWN, "4242");
    let unknown = target(OWN, "9999");
    let session = session_of(&[(5, planet), (10, &known), (20, &unknown)]);
    let targets: Vec<Option<&str>> = of(&session, Kind::Target).iter().map(|e| e.data.target.as_deref()).collect();
    assert_eq!(targets, [Some("nyx1"), None], "an unknown number names nothing");
}

#[test]
fn the_target_key_loses_prefix_and_serial() {
    let a = target(OWN, "LOC_rs_ext_pyro6_leo_12345678");
    let b = target(OWN, "MISSION_QT_Quantum_Beacon_TSG_1");
    let session = session_of(&[(10, &a), (20, &b)]);
    let targets: Vec<Option<&str>> = of(&session, Kind::Target).iter().map(|e| e.data.target.as_deref()).collect();
    assert_eq!(targets, [Some("rs_ext_pyro6_leo"), Some("MISSION_QT")]);
}

#[test]
fn the_start_is_taken_once_per_target() {
    let first = target(OWN, "LOC_rs_ext_pyro6_leo_12345678");
    let second = target(OWN, "LOC_rs_ext_nyx-castra_jp1");
    let session = session_of(&[
        (10, &first),
        (11, "<Calculate Route> Projected Start Location is  Levski  for route to destination 1"),
        (12, "<Calculate Route> Projected Start Location is Terminus for route to destination 1"),
        (20, &second),
        (21, "<Calculate Route> Projected Start Location is Orison for route to destination 2"),
    ]);
    let starts: Vec<Option<&str>> = of(&session, Kind::Target).iter().map(|e| e.data.from.as_deref()).collect();
    assert_eq!(starts, [Some("Levski"), Some("Orison")], "only the first calculation after a choice counts");
    let lone = session_of(&[(11, "<Calculate Route> Projected Start Location is Levski for route")]);
    assert!(of(&lone, Kind::Target).is_empty(), "a route nobody chose is no target");
}

#[test]
fn a_lost_connection_is_one_event_and_the_light_one_none() {
    let session = session_of(&[
        (3, "<Channel Process Disconnection> cause=30000 reason=\"light\" DisconnectLight"),
        (100, "[Notice] <Channel Process Disconnection> cause=30000 reason=\"CNetChannel::InactivityTimerCallback connection has timed out\""),
        (100, "[Notice] <Error Popup Opened> errorCode=30000"),
        (102, "[Notice] <Error Popup Opened> errorCode=30000"),
        (110, "[Notice] <Channel Process Disconnection> cause=1234 reason=\"other\""),
        (400, "[Notice] <Error Popup Opened> errorCode=30000"),
    ]);
    let times: Vec<i64> = of(&session, Kind::Connection).iter().map(|e| e.at - session.start).collect();
    assert_eq!(times, [100, 400], "the two lines of one loss are one event, a later one is another");
}

#[test]
fn a_death_needs_the_own_body_port_first() {
    let own = attachment("Xharig", "777");
    let other = attachment("Fremd", "555");
    let (c1, c2, c3, c4) = (corpse("888"), corpse("777"), corpse("777"), corpse("777"));
    let (alien, c5) = (corpse("555"), corpse("777"));
    let session = session_of(&[
        (50, &c1),
        (60, &own),
        (70, &other),
        (100, &c2),
        (105, &c3),
        (110, &alien),
        (130, &c4),
        (131, &c5),
    ]);
    let deaths: Vec<i64> = of(&session, Kind::Death).iter().map(|e| e.at - session.start).collect();
    assert_eq!(deaths, [100, 130], "no body before the port, another player's body never, one corpse block is one death");
}

#[test]
fn the_statistics_and_the_flight_log_count_the_same_deaths() {
    let own = attachment("Xharig", "777");
    let (c1, c2, c3) = (corpse("777"), corpse("777"), corpse("777"));
    let lines = [(60, own.as_str()), (100, c1.as_str()), (105, c2.as_str()), (140, c3.as_str())];
    let text = log(&lines);
    let session = read_log_text(&text, Some("Xharig"), &Phrases::table()).expect("a session");
    let stats = crate::play_stats::read_log_text(&text, Some("Xharig")).expect("a session");
    let Some(Value::Array(moments)) = get(&stats, "tode") else { panic!("tode") };
    assert_eq!(moments.len(), of(&session, Kind::Death).len());
    assert_eq!(moments.len(), 2);
}

#[test]
fn no_address_no_handle_no_id_is_kept() {
    let join = "[Notice] <Join PU> address[35.241.242.86] port[64388] shard[pub_euw1b_12881860_420] locationId[5629]";
    let mission = "<EndMission> a MissionId[11111111-aaaa-4bbb-8ccc-000000000001] Player[Xharig] PlayerId[4815162342] CompletionType[Complete]";
    let strangers = "<Trying to QT when not in Idle> User: FremdHandle Position_ZS 1.5 2.5 accountId[99887766] geid[44556677]";
    let pick = target(OWN, "LOC_rs_ext_nyx-castra_jp1");
    let session = session_of(&[(5, join), (6, mission), (7, strangers), (10, &pick)]);
    let text = jsonio::dumps(&Value::Object(session.to_map()), None, false);
    for secret in ["35.241.242.86", "64388", "locationId", "5629", "FremdHandle", "4815162342", "11111111-aaaa", "99887766", "44556677", "Position_ZS"] {
        assert!(!text.contains(secret), "{secret} stands in the file");
    }
    assert!(text.contains("pub_euw1b_12881860_420"), "the shard stays, on this computer only");
}

#[test]
fn a_join_is_a_shard_event_and_a_change_of_server_is_one() {
    let join = "[Notice] <Join PU> address[1.2.3.4] port[1] shard[pub_euw1b_12881860_420] locationId[1]";
    let session = session_of(&[(5, join), (20, "<Change Server Start> to somewhere")]);
    assert_eq!(of(&session, Kind::Shard)[0].data.shard.as_deref(), Some("pub_euw1b_12881860_420"));
    assert_eq!(of(&session, Kind::ServerChange).len(), 1);
}

#[test]
fn a_line_without_a_stamp_tells_nothing_about_when_and_makes_no_event() {
    let unstamped = [
        change("0", "123"),
        target(OWN, "LOC_rs_ext_nyx-castra_jp1"),
        arrival(OWN),
        "<Channel Process Disconnection> cause=30000 reason=\"x\"".to_string(),
        "<Change Server Start> x".to_string(),
        "<SystemQuit>".to_string(),
    ];
    let mut text = format!("{} Start OnClientSpawned\n", stamp(0));
    for line in &unstamped {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(&format!("{} Ende\n", stamp(1800)));
    let session = read_log_text(&text, Some("Xharig"), &Phrases::table()).expect("a session");
    assert_eq!(session.events.len(), 1, "only the start: {:?}", session.events);
    assert!(session.events.iter().all(|e| e.at > 0));
}

#[test]
fn the_tile_of_server_changes_counts_what_the_switch_of_servers_shows() {
    let join = |shard: &str| format!("[Notice] <Join PU> address[1.2.3.4] port[1] shard[{shard}] locationId[1]");
    let (a, b, c) = (join("pub_euw1b_12881860_420"), join("pub_euw1b_12881860_420"), join("pub_euw1b_12881999_17"));
    let session = session_of(&[(5, &a), (10, &b), (15, "<Change Server Start> x"), (20, &c)]);
    let book = Book::of_sessions(vec![session.clone()]);
    // The first join is none, the same shard again none, another shard one, the change of server one.
    assert_eq!(book.summary(&session).server_changes, 2);
}

#[test]
fn one_end_one_crash_and_the_loading_screen_with_its_time() {
    let session = session_of(&[
        (10, "<SystemQuit>"),
        (11, "<SystemQuit>"),
        (20, "crash handler taking over"),
        (21, "-- GPU CRASH"),
        (30, "Loading screen for Nyx : SC_Frontend closed after 24.79 seconds"),
    ]);
    assert_eq!((of(&session, Kind::End).len(), of(&session, Kind::Crash).len()), (1, 1));
    let loading = of(&session, Kind::Loading);
    assert_eq!((loading[0].data.name.as_deref(), loading[0].data.seconds), (Some("Nyx"), Some(25)));
}

#[test]
fn an_injury_counts_once_within_ten_seconds_and_by_its_severity() {
    let note = |text: &str| format!("[Notice] <SHUDEvent_OnNotification> Added notification \"{text}: \" [3]");
    let (a, b, c, d) = (note("Minor Injury Detected"), note("Minor Injury Detected"), note("Severe Injury Detected"), note("Bauplan erhalten: Helm"));
    let session = session_of(&[(10, &a), (14, &b), (50, &c), (60, &d)]);
    let hurt: Vec<(i64, &str)> =
        of(&session, Kind::Injury).iter().map(|e| (e.at - session.start, e.data.severity.as_deref().unwrap_or(""))).collect();
    assert_eq!(hurt, [(10, "leicht"), (50, "schwer")]);
}

#[test]
fn a_session_over_midnight_keeps_its_order_and_its_times() {
    let text = "<2026-10-01T23:30:00.000Z> Start OnClientSpawned\n\
                <2026-10-01T23:59:59.000Z> <Update Inventory Location> Player [Xharig] is changing location. Landing [0] -> [1]. Location [0] -> [5]\n\
                <2026-10-02T00:00:01.000Z> <Update Inventory Location> Player [Xharig] is changing location. Landing [1] -> [0]. Location [0] -> [5]\n\
                <2026-10-02T00:30:00.000Z> Ende\n";
    let session = read_log_text(text, None, &Phrases::table()).expect("a session");
    assert_eq!(session.end - session.start, 3600);
    let places: Vec<i64> = of(&session, Kind::Place).iter().map(|e| e.at - session.start).collect();
    assert_eq!(places, [1799, 1801]);
}

#[test]
fn a_session_is_written_and_read_back_the_same() {
    let pick = target(OWN, "LOC_rs_ext_nyx-castra_jp1");
    let mut lines: Vec<(i64, String)> = vec![(10, pick), (20, arrival(OWN)), (30, change("0", "123")), (40, "<SystemQuit>".to_string())];
    lines.push((50, "Loading screen for pu : SC_Frontend closed after 3.2 seconds".to_string()));
    let refs: Vec<(i64, &str)> = lines.iter().map(|(at, text)| (*at, text.as_str())).collect();
    let session = session_of(&refs);
    let text = jsonio::dumps(&Value::Object(session.to_map()), None, false);
    let Ok(Value::Object(map)) = jsonio::loads(&text) else { panic!("an object") };
    assert_eq!(Session::from_map(&map), Some(session));
}

#[test]
fn an_event_of_a_kind_this_version_does_not_know_is_skipped() {
    let known = Value::Object(vec![("wann".into(), Value::int(5)), ("art".into(), Value::str("ort")), ("ort".into(), Value::str("7"))]);
    let unknown = Value::Object(vec![("wann".into(), Value::int(6)), ("art".into(), Value::str("kommt-spaeter"))]);
    assert_eq!(Event::from_value(&unknown), None);
    let event = Event::from_value(&known).expect("an event");
    assert_eq!((event.kind, event.data.place.as_deref()), (Kind::Place, Some("7")));
}

#[test]
fn a_log_in_pieces_gives_the_same_events() {
    let planet = "<Planet CPU Maps Loaded> x Planet 'nyx1' (entity 4242) y".to_string();
    let a = target(OWN, "4242");
    let b = arrival(OWN);
    let c = change("0", "123");
    let d = attachment("Xharig", "777");
    let e = corpse("777");
    let lines = [(5, planet.as_str()), (10, &a), (20, &b), (30, &c), (40, &d), (50, &e), (60, "<SystemQuit>")];
    let text = log(&lines);
    let whole = read_log_text(&text, Some("Xharig"), &Phrases::table()).expect("a session");
    // Line by line, as the pieces of a big log come.
    let phrases = Phrases::table();
    let mut scan = Scan::new(Some("Xharig"), &phrases);
    for line in text.split_inclusive('\n') {
        scan.piece(line);
    }
    assert_eq!(scan.finish(), Some(whole.clone()));
    // From a file, with a line across the border of one block.
    let (paths, root) = paths_in("teile");
    let _ = paths;
    let file = root.join("Game.log");
    let mut filled = String::new();
    let flood = format!("{} [Notice] <Monitored Space> \"Ueberwachter Raum ausgefallen: \" [4711]\n", stamp(1));
    let want = logread::BLOCK - 40;
    while filled.len() + flood.len() < want {
        filled.push_str(&flood);
    }
    filled.push_str(&"x".repeat(want - filled.len() - 1));
    filled.push('\n');
    let mut big = format!("{} Start OnClientSpawned\n", stamp(0));
    big.push_str(&filled);
    for (at, text) in &lines {
        big.push_str(&format!("{} {text}\n", stamp(*at)));
    }
    big.push_str(&format!("{} Ende\n", stamp(1800)));
    fs::write(&file, &big).expect("file");
    let from_file = read_log(file.to_str().expect("path"), Some("Xharig"), &Phrases::table()).expect("a session");
    assert_eq!(from_file, whole, "the events do not depend on where the pieces end");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_huge_session_is_capped_and_keeps_the_rare_events() {
    let mut text = format!("{} Start OnClientSpawned\n", stamp(0));
    for n in 0..(3 * MAX_EVENTS) as i64 {
        text.push_str(&format!("{} Loading screen for Nyx : SC_Frontend closed after 1.0 seconds\n", stamp(1 + n % 1000)));
    }
    let own = attachment("Xharig", "777");
    text.push_str(&format!("{} {own}\n", stamp(1100)));
    text.push_str(&format!("{} {}\n", stamp(1200), corpse("777")));
    text.push_str(&format!("{} {}\n", stamp(1300), change("0", "9")));
    text.push_str(&format!("{} <SystemQuit>\n{} Ende\n", stamp(1400), stamp(1800)));
    let session = read_log_text(&text, Some("Xharig"), &Phrases::table()).expect("a session");
    assert!(session.events.len() <= MAX_EVENTS, "{}", session.events.len());
    for kind in [Kind::Start, Kind::End, Kind::Death, Kind::Place] {
        assert_eq!(session.count(kind), 1, "{kind:?} stays");
    }
    assert!(session.count(Kind::Loading) < 3 * MAX_EVENTS, "the loading screens go first");
}

#[test]
fn the_cap_drops_the_often_kind_first_and_the_oldest_of_it() {
    let event = |at: i64, kind: Kind| Event::new(at, kind);
    let mut events: Vec<Event> = (0..MAX_EVENTS as i64).map(|n| event(n, Kind::Loading)).collect();
    events.extend((0..10).map(|n| event(10_000 + n, Kind::Death)));
    events.push(event(20_000, Kind::Place));
    cap(&mut events);
    assert_eq!(events.len(), MAX_EVENTS);
    assert_eq!(events.iter().filter(|e| e.kind == Kind::Death).count(), 10);
    assert_eq!(events.iter().filter(|e| e.kind == Kind::Place).count(), 1);
    assert_eq!(events.iter().find(|e| e.kind == Kind::Loading).map(|e| e.at), Some(11), "the oldest ones went");
}

#[test]
fn an_older_format_is_read_again() {
    let (paths, root) = paths_in("format");
    let old = "{\"format\": 0, \"sitzungen\": {\"100\": {\"von\": 100, \"bis\": 200, \"account\": \"\", \"ereignisse\": []}}, \"gelesen\": {\"a.log\": 5}}";
    fs::create_dir_all(Flavor::native().dirname(&paths.app_file(FILE))).expect("folder");
    fs::write(paths.app_file(FILE), old).expect("file");
    let data = load(&paths);
    assert_eq!(get(&data, "gelesen"), Some(&Value::Object(Vec::new())), "every log is read once more");
    assert_eq!(sessions(&paths).len(), 1, "what is kept stays");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_catch_up_reads_a_log_once_and_a_rescan_keeps_the_sessions() {
    let (paths, root) = paths_in("nachholen");
    let logs = root.join("logs");
    fs::create_dir_all(&logs).expect("folder");
    let pick = target(OWN, "LOC_rs_ext_nyx-castra_jp1");
    let file = logs.join("Game.log");
    fs::write(&file, log(&[(10, &pick)])).expect("file");
    let files = vec![file.to_str().expect("path").to_string()];
    let src = LogSource::new(&paths, LogSource::sources_of(&paths));
    assert_eq!(catch_up(&src, &files), 1);
    assert_eq!(catch_up(&src, &files), 0, "nothing new, nothing read");
    assert_eq!(sessions(&paths).len(), 1);
    fs::remove_file(&file).expect("gone");
    assert_eq!(rescan(&src, &[]), 0);
    assert_eq!(sessions(&paths).len(), 1, "what vanished from the logs keeps its events");
    let _ = fs::remove_dir_all(&root);
}

fn entry(name: &str, accepted: &str, ended: Option<&str>, state: &str, blueprints: &[&str]) -> Map {
    let mut map: Map = vec![("name".into(), Value::str(name)), ("wann".into(), Value::str(accepted))];
    if let Some(end) = ended {
        map.push(("bis".into(), Value::str(end)));
    }
    map.push(("zustand".into(), Value::str(state)));
    map.push(("bauplaene".into(), Value::Array(blueprints.iter().map(|b| Value::str(*b)).collect())));
    map
}

#[test]
fn the_timeline_mixes_contracts_in_time_order() {
    let (paths, root) = paths_in("zeitstrahl");
    location::remember(&paths, &vec![("123".to_string(), "Nyx_Levski".to_string())]);
    mission_log::save(
        &paths,
        &[
            entry("Zeta", "2026-10-01T10:05:00", Some("2026-10-01T10:10:00"), mission_log::COMPLETED, &["Helm"]),
            entry("Alpha", "2026-10-01T10:12:00", None, mission_log::RUNNING, &[]),
            entry("Outside", "2026-10-02T10:05:00", None, mission_log::RUNNING, &[]),
        ],
    );
    let pick = target(OWN, "LOC_rs_ext_nyx-castra_jp1");
    let there = change("0", "123");
    let away = change("123", "0");
    let session = session_of(&[(100, &there), (400, &pick), (700, &away)]);
    let mut book = Book::load(&paths);
    book.sessions = vec![session.clone()];
    let rows = book.rows(&session);
    let label = |row: &Row| match &row.item {
        Item::Event(event) => event.kind.stored().to_string(),
        Item::Accepted { name } => format!("angenommen {name}"),
        Item::Ended { name, state } => format!("beendet {name} {state}"),
        Item::Find { name, contract } => format!("fund {name} von {contract}"),
    };
    let order: Vec<String> = rows.iter().map(|r| format!("{} {}", r.at - session.start, label(r))).collect();
    assert_eq!(
        order,
        [
            "0 start",
            "100 ort",
            "300 angenommen Zeta",
            "400 ziel",
            "600 beendet Zeta abgeschlossen",
            "600 fund Helm von Zeta",
            "700 ort",
            "720 angenommen Alpha",
        ],
        "by time, not by name; a contract still running has no end; one of another day is not in this session"
    );
    let where_of = |at: i64| rows.iter().find(|r| r.at - session.start == at).map(|r| r.place.clone());
    assert_eq!(where_of(0), Some(Where::Unknown));
    assert_eq!(where_of(300), Some(Where::At(Some("Nyx Levski".to_string()))));
    assert_eq!(where_of(720), Some(Where::Underway));
    let summary = book.summary(&session);
    assert_eq!((summary.contracts, summary.finds, summary.places, summary.duration), (2, 1, 2, 1800));
    let _ = fs::remove_dir_all(&root);
}
