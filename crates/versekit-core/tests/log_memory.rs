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

//! The memory the readers of a whole Game.log need: the evaluation of the playing statistics
//! (`play_stats::read_log`, `playtime::span_from_log`) and the start of the overlay (`live_contracts::StartLog`).
//!
//! A counting allocator gives the peak of the heap while a reader runs. The log is a made-up one in the
//! temp folder: a session with contracts, a change of server and joins, between them lines of the flood the
//! game writes. The old way (the whole file in memory, then the `*_text` functions) stays in this file as the
//! one the new readers are compared with.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use versekit_core::contracts::Patterns;
use versekit_core::jsonio::Map;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::live_contracts::{Check, Line, Live, Out, StartLog, hidden_load};
use versekit_core::logscan::names_from_text;
use versekit_core::logsource::{LogSource, LogTail, ReadState};
use versekit_core::mission_log::{ContractBlueprints, ContractIndex};
use versekit_core::mission_scan::MissionScan;
use versekit_core::paths::{ENV_INSTALL_DIR, Host, Paths};
use versekit_core::phrases::{Sources, pattern};
use versekit_core::pypath::Flavor;
use versekit_core::shard::{Join, joins_in};
use versekit_core::text::{decode_ignore, decode_replace};
use versekit_core::{contracts, legacy, logread, logscan, play_stats, playtime};

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

/// The system allocator that counts the bytes in use and their peak.
struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(now, Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { System.realloc(ptr, layout, new_size) };
        if !moved.is_null() {
            if new_size >= layout.size() {
                let more = new_size - layout.size();
                let now = CURRENT.fetch_add(more, Ordering::SeqCst) + more;
                PEAK.fetch_max(now, Ordering::SeqCst);
            } else {
                CURRENT.fetch_sub(layout.size() - new_size, Ordering::SeqCst);
            }
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The most the heap held above what it held before `run`, while `run` ran.
fn peak_of<T>(run: impl FnOnce() -> T) -> (T, usize) {
    let before = CURRENT.load(Ordering::SeqCst);
    PEAK.store(before, Ordering::SeqCst);
    let out = run();
    (out, PEAK.load(Ordering::SeqCst).saturating_sub(before))
}

/// The tests of this file share the counters: one at a time.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

const MB: usize = 1 << 20;

/// How a made-up log is written.
#[derive(Clone, Copy)]
struct Style {
    /// Line ends are `\r\n`.
    crlf: bool,
    /// Bytes that are no UTF-8 stand between the lines.
    broken_bytes: bool,
    /// The last line has no end.
    open_end: bool,
}

const PLAIN: Style = Style { crlf: false, broken_bytes: false, open_end: false };

const MISSION_A: &str = "11111111-aaaa-4bbb-8ccc-000000000001";
const MISSION_B: &str = "22222222-aaaa-4bbb-8ccc-000000000002";
const MISSION_C: &str = "33333333-aaaa-4bbb-8ccc-000000000003";
const CONTRACT: &str = "99999999-aaaa-4bbb-8ccc-0000000000ff";

/// One line of the made-up log: stamp, text, line end.
fn put(out: &mut impl Write, end: &str, stamp: &str, text: &str) {
    write!(out, "<2026-11-05T{stamp}.000Z> {text}{end}").expect("write");
}

/// The made-up log: `megabytes` of flood between the lines the readers look at.
fn write_log(path: &Path, megabytes: usize, style: Style) {
    let end = if style.crlf { "\r\n" } else { "\n" };
    let mut out = BufWriter::new(File::create(path).expect("log"));
    let notice = "[Notice] <SHUDEvent_OnNotification> Added notification";
    let opening = [
        ("22:27:20", "Log started on Thu Nov  5 22:27:20 2026".to_string()),
        ("22:27:30", "[Notice] <CSessionManager::OnClientSpawned> Spawned!".to_string()),
        (
            "22:27:31",
            "[Notice] <Join PU> address[35.241.242.86] port[64388] shard[pub_euw1b_12881860_420] locationId[5629]"
                .to_string(),
        ),
        ("22:27:32", "<CLoginSession> x Fully authenticated : handle[Other]".to_string()),
        (
            "22:28:00",
            format!(
                "{notice} \"Auftrag angenommen: Hunt the Target: \" [1] MissionId: [{MISSION_A}], ObjectiveId: [x1]"
            ),
        ),
        (
            "22:28:01",
            format!("{notice} \"Auftrag angenommen: Second Job: \" [1] MissionId: [{MISSION_B}], ObjectiveId: [x2]"),
        ),
        (
            "22:28:02",
            format!("{notice} \"Auftrag angenommen: Third Job: \" [1] MissionId: [{MISSION_C}], ObjectiveId: [x3]"),
        ),
        ("22:28:03", format!("CreateMarker something missionId [{MISSION_A}] and contractDefinitionId[{CONTRACT}]")),
        (
            "22:28:04",
            format!(
                "<ObjectiveUpserted> x mission_id {MISSION_A} - objective_id obj1 - state MISSION_OBJECTIVE_STATE_INPROGRESS flags=ShowInLog"
            ),
        ),
        (
            "22:28:05",
            format!(
                "{notice} \"Neues Ziel: Fly to the moon: \" [1] MissionId: [{MISSION_A}], ObjectiveId: [aaaaaaaa-1111-2222-3333-444444444444]"
            ),
        ),
    ];
    for (stamp, text) in &opening {
        put(&mut out, end, stamp, text);
    }
    // The flood: two lines the game writes millions of times, and now and then one the readers look at.
    let flood_a = "[Notice] <Monitored Space> \"Überwachter Raum ausgefallen: \" [4711]";
    let flood_b = "[Notice] <Monitored Space> \"Überwachter Raum wiederhergestellt: \" [4711]";
    let each = flood_a.len() + flood_b.len() + 2 * 27 + 2 * end.len();
    let rounds = (megabytes * MB) / each;
    for round in 0..rounds {
        put(&mut out, end, "22:30:00", flood_a);
        put(&mut out, end, "22:30:01", flood_b);
        if style.broken_bytes && round % 997 == 0 {
            out.write_all(b"<2026-11-05T22:30:02.000Z> broken \xff\xfe bytes \xc3 in a line").expect("write");
            out.write_all(end.as_bytes()).expect("write");
        }
        if round % 5003 == 0 {
            put(&mut out, end, "22:30:59", &format!("{notice} \"Bauplan erhalten: Teil {round}: \" [1]"));
            put(&mut out, end, "22:31:00", "<Quantum Drive Arrived - Arrived at Final Destination>");
            put(&mut out, end, "22:31:01", "<EndMission> a MissionId[m1] Player[Xharig] CompletionType[Complete]");
        }
        // After the change of server only the first contract is named again, in a line of no kind.
        if round == rounds / 2 {
            put(&mut out, end, "23:00:00", "<Change Server Start> to somewhere");
            put(&mut out, end, "23:00:01", &format!("[Notice] some other kind of line that names {MISSION_A} again"));
        }
        if round == rounds * 3 / 4 {
            put(
                &mut out,
                end,
                "23:10:00",
                "[Notice] <Join PU> address[35.241.242.87] port[64389] shard[pub_euw1b_12881999_17] locationId[5629]",
            );
            put(&mut out, end, "23:10:01", "Überwachte Ünïcode-Zeile mit <ObjectiveUpserted> ohne Zahlen");
        }
    }
    put(&mut out, end, "23:58:00", "<Legacy login response> [CIG-net] User Login Success - Handle[Xharig] - Time[1]");
    put(&mut out, end, "23:59:00", "<EndMission> a MissionId[m2] Player[Xharig] CompletionType[Fail]");
    put(&mut out, end, "23:59:30", "<SystemQuit>");
    if style.open_end {
        write!(out, "<2026-11-05T23:59:59.000Z> the last line without an end").expect("write");
    }
    out.flush().expect("flush");
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("versekit-logmemory-{}-{name}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    dir
}

fn paths_in(dir: &Path) -> Paths {
    let home = dir.to_string_lossy().into_owned();
    Paths::new(Host::new(Flavor::native(), &[(legacy::ENV_HOME, home.as_str())], &home))
}

fn catalog() -> ContractBlueprints {
    ContractBlueprints { index: ContractIndex::default(), missions: HashMap::new(), definitions: HashMap::new() }
}

/// What the start of the overlay makes of a log: the messages, the bar, the joins and the handle.
type Started = (Vec<Out>, Vec<Line>, Vec<Join>, Option<String>);

/// The start as it was: the whole file in memory, then the state of the contracts from the text.
fn start_whole(paths: &Paths, log: &Path) -> Started {
    let bytes = std::fs::read(log).expect("log");
    let text = decode_ignore(&bytes);
    let patterns = Patterns::of(&contracts::start_phrases(&[]), &contracts::end_phrases(&[]));
    let (blueprints, has) = (catalog(), |_: &str| false);
    let check = Check { blueprints: &blueprints, has: &has };
    let (mut live, mut out) = (Live::new(), Vec::new());
    live.at_start(&text, &patterns, &check, &hidden_load(paths), &mut out);
    (out, live.stand(), joins_in(&text), logscan::account_from_text(&text))
}

/// The start as it is: a piece at a time.
fn start_pieces(paths: &Paths, log: &Path) -> Started {
    let read = StartLog::read(&log.to_string_lossy()).expect("log");
    let patterns = Patterns::of(&contracts::start_phrases(&[]), &contracts::end_phrases(&[]));
    let (blueprints, has) = (catalog(), |_: &str| false);
    let check = Check { blueprints: &blueprints, has: &has };
    let (mut live, mut out) = (Live::new(), Vec::new());
    live.at_start_log(&read, &patterns, &check, &hidden_load(paths), &mut out);
    (out, live.stand(), joins_in(read.lines()), read.handle())
}

/// The session entry of the statistics as it was: the whole file in memory.
fn stats_whole(log: &Path, account: Option<&str>) -> Option<Map> {
    let bytes = std::fs::read(log).ok()?;
    play_stats::read_log_text(&decode_replace(&bytes), account)
}

/// The playing time of a log as it was: the whole file in memory.
fn spans_whole(log: &Path) -> Option<(i64, i64)> {
    let bytes = std::fs::read(log).ok()?;
    playtime::span_of_text(&decode_replace(&bytes))
}

#[test]
fn the_readers_find_what_the_whole_file_gives() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    set_thread_language(Choice::De);
    let dir = scratch("gleich");
    let paths = paths_in(&dir);
    let styles = [
        PLAIN,
        Style { crlf: true, ..PLAIN },
        Style { broken_bytes: true, ..PLAIN },
        Style { crlf: true, broken_bytes: true, open_end: true },
    ];
    for (index, style) in styles.iter().enumerate() {
        // Sizes around the block: none, a little, a little over one, several.
        for megabytes in [0, 1, logread::BLOCK / MB + 1, 3 * logread::BLOCK / MB + 1] {
            let log = dir.join(format!("Game-{index}-{megabytes}.log"));
            write_log(&log, megabytes, *style);
            let path = log.to_string_lossy().into_owned();
            let old = start_whole(&paths, &log);
            let new = start_pieces(&paths, &log);
            assert_eq!(old.0, new.0, "messages of {path}");
            assert_eq!(old.1, new.1, "bar of {path}");
            assert_eq!(old.2, new.2, "joins of {path}");
            assert_eq!(old.3, new.3, "handle of {path}");
            assert_eq!(old.3.as_deref(), Some("Xharig"), "the first expression wins, wherever its line stands");
            assert_eq!(old.2.len(), if megabytes == 0 { 1 } else { 2 }, "the joins of the made-up log");
            assert_eq!(old.1.len(), 3, "the made-up log leaves three contracts open");
            let span = spans_whole(&log);
            assert_eq!(span, playtime::span_from_log(&path), "playing time of {path}");
            assert!(span.is_some(), "the player arrived in {path}");
            for account in [None, Some("Xharig"), Some("Other")] {
                let whole = stats_whole(&log, account);
                assert_eq!(whole, play_stats::read_log(&path, account), "statistics of {path} for {account:?}");
                assert!(whole.is_some(), "the player arrived in {path}");
            }
            std::fs::remove_file(&log).expect("remove");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_contract_that_fell_silent_after_the_change_of_server_turns_grey() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    set_thread_language(Choice::De);
    let dir = scratch("grau");
    let paths = paths_in(&dir);
    let log = dir.join("Game.log");
    write_log(&log, 1, PLAIN);
    let (_, bar, _, _) = start_pieces(&paths, &log);
    let grey: Vec<(&str, bool)> = bar.iter().map(|line| (line.title.as_str(), line.grey)).collect();
    // The first contract is named again after the change, in a line of no kind the start keeps; the others are not.
    assert_eq!(grey, [("Hunt the Target", false), ("Second Job", true), ("Third Job", true)]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_peak_does_not_grow_with_the_log() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    set_thread_language(Choice::De);
    let dir = scratch("klein");
    let paths = paths_in(&dir);
    for megabytes in [24, 96] {
        let log = dir.join(format!("Game-{megabytes}.log"));
        write_log(&log, megabytes, PLAIN);
        let path = log.to_string_lossy().into_owned();
        let (_, start) = peak_of(|| start_pieces(&paths, &log));
        let (_, stats) = peak_of(|| play_stats::read_log(&path, Some("Xharig")));
        let (_, time) = peak_of(|| playtime::span_from_log(&path));
        // A few blocks at the most, whatever the size of the log.
        assert!(start <= 3 * logread::BLOCK, "start of the overlay, {megabytes} MB: {start}");
        assert!(stats <= 3 * logread::BLOCK, "statistics, {megabytes} MB: {stats}");
        assert!(time <= 3 * logread::BLOCK, "playing time, {megabytes} MB: {time}");
        std::fs::remove_file(&log).expect("remove");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A made-up game folder in `dir` with the log `Game.log` of `megabytes` and an empty `logbackups`.
fn game_in(dir: &Path, megabytes: usize) -> (Paths, PathBuf) {
    let home = dir.join("home");
    let game = dir.join("StarCitizen").join("LIVE");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(game.join("logbackups")).expect("backups");
    let log = game.join("Game.log");
    write_log(&log, megabytes, PLAIN);
    let (home, game) = (home.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
    let host = Host::new(
        Flavor::native(),
        &[(legacy::ENV_HOME, home.as_str()), (ENV_INSTALL_DIR, game.as_str())],
        &dir.to_string_lossy(),
    );
    (Paths::new(host), log)
}

/// The blueprints the tail finds in a log that grew while nobody read it: the remembered position lies at the
/// start. The old tail read everything at once, the new one a block at a time, so it is asked often enough.
fn tail_finds(paths: &Paths, log: &Path) -> Vec<(String, Option<logscan::Extra>)> {
    let src = LogSource::new(paths, Sources::default());
    let mut state = ReadState::open(paths);
    state.set_active(&log.to_string_lossy(), 0, &|| 0.0);
    let mut tail = LogTail::new(&src, state, pattern(&["Bauplan erhalten".into()]));
    let calls = std::fs::metadata(log).expect("log").len() as usize / logread::BLOCK + 3;
    (0..calls).flat_map(|_| tail.new_names()).collect()
}

#[test]
fn the_tail_finds_what_the_whole_text_has_and_stays_small() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = scratch("schwanz");
    for megabytes in [1, 3 * logread::BLOCK / MB + 1, 40] {
        let (paths, log) = game_in(&dir.join(format!("{megabytes}")), megabytes);
        let text = decode_ignore(&std::fs::read(&log).expect("log"));
        let want = names_from_text(&text, &pattern(&["Bauplan erhalten".into()]), &Sources::default());
        drop(text);
        let (found, peak) = peak_of(|| tail_finds(&paths, &log));
        assert_eq!(found, want, "the blueprints of {megabytes} MB");
        assert!(!want.is_empty());
        assert!(peak <= 4 * logread::BLOCK, "tail, {megabytes} MB: {peak}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The reading of the contract log from the start to the end: the playing time, the statistics, the stock journal
/// and the history of the contracts together.
#[test]
fn the_whole_backlog_reading_stays_small_whatever_the_size_of_the_log() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    set_thread_language(Choice::De);
    let dir = scratch("nachlesen");
    for megabytes in [24, 96] {
        let (paths, _) = game_in(&dir.join(format!("{megabytes}")), megabytes);
        let src = LogSource::new(&paths, Sources::default());
        let scan = MissionScan::new(&src);
        let (result, peak) = peak_of(|| scan.scan_backlog());
        assert!(result.is_ok(), "{result:?}");
        assert!(peak <= 8 * logread::BLOCK, "backlog reading, {megabytes} MB: {peak}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The measurement for the report: `cargo test --release -p versekit-core --test log_memory -- --ignored --nocapture`.
/// The size comes from `LOG_MEMORY_MB` (500 without it).
#[test]
#[ignore = "writes a log of hundreds of megabytes into the temp folder"]
fn measure_a_big_log() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    set_thread_language(Choice::De);
    let megabytes: usize = std::env::var("LOG_MEMORY_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(500);
    let dir = scratch("gross");
    let paths = paths_in(&dir);
    let log = dir.join("Game.log");
    write_log(&log, megabytes, PLAIN);
    let size = std::fs::metadata(&log).expect("log").len() as usize;
    let path = log.to_string_lossy().into_owned();
    let mb = |bytes: usize| bytes as f64 / MB as f64;
    println!("log: {:.0} MB", mb(size));
    let (_, whole) = peak_of(|| start_whole(&paths, &log));
    println!("overlay start, whole file : peak {:.1} MB", mb(whole));
    let (_, pieces) = peak_of(|| start_pieces(&paths, &log));
    println!("overlay start, pieces     : peak {:.1} MB", mb(pieces));
    let (_, whole) = peak_of(|| stats_whole(&log, Some("Xharig")));
    println!("play_stats, whole file    : peak {:.1} MB", mb(whole));
    let (_, pieces) = peak_of(|| play_stats::read_log(&path, Some("Xharig")));
    println!("play_stats, pieces        : peak {:.1} MB", mb(pieces));
    let (_, whole) = peak_of(|| spans_whole(&log));
    println!("playtime, whole file      : peak {:.1} MB", mb(whole));
    let (_, pieces) = peak_of(|| playtime::span_from_log(&path));
    println!("playtime, pieces          : peak {:.1} MB", mb(pieces));
    // The old tail took the rest of the file in one go and decoded it, the old history read each log whole.
    let old_tail = |log: &Path| {
        let text = decode_ignore(&std::fs::read(log).expect("log"));
        names_from_text(&text, &pattern(&["Bauplan erhalten".into()]), &Sources::default()).len()
    };
    let (_, whole) = peak_of(|| old_tail(&log));
    println!("tail, old way             : peak {:.1} MB", mb(whole));
    let (_, whole) = peak_of(|| std::fs::read(&log).map(|bytes| decode_replace(&bytes).len()));
    println!("history/titles, old way   : peak {:.1} MB (per log, up to 16 logs at once)", mb(whole));
    std::fs::remove_file(&log).expect("remove");
    let (game_paths, game_log) = game_in(&dir.join("game"), megabytes);
    let (_, tail) = peak_of(|| tail_finds(&game_paths, &game_log));
    println!("tail, catching up         : peak {:.1} MB", mb(tail));
    let src = LogSource::new(&game_paths, Sources::default());
    let scan = MissionScan::new(&src);
    let (result, backlog) = peak_of(|| scan.scan_backlog());
    println!("backlog reading ({result:?}) : peak {:.1} MB", mb(backlog));
    let _ = std::fs::remove_dir_all(&dir);
}
