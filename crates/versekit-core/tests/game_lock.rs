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

//! The lock of the game files across processes (`injection::share_game_files_between_processes`): the overlay and
//! the main window both read a `global.ini` with its note of original texts and write both back. Under the shared
//! lock a writer never sees the file of one writing with the note of another, and no writing is lost.
//!
//! The test starts itself again as the other process (`child_entry`, told what to do by variables).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use versekit_core::injection;
use versekit_core::jsonio::{self, Value};
use versekit_core::testdir::Dir;

const ROLE: &str = "VK_GAME_LOCK_ROLE";
const DIR: &str = "VK_GAME_LOCK_DIR";
const NAME: &str = "VK_GAME_LOCK_NAME";
const COUNT: &str = "VK_GAME_LOCK_COUNT";
const SHARE: &str = "VK_GAME_LOCK_SHARE";
const HOLD_MS: &str = "VK_GAME_LOCK_HOLD_MS";

/// How often each of the two processes writes the pair.
const WRITINGS: usize = 40;

static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_file(dir: &Path) -> String {
    dir.join("sperre-spiel.lock").to_string_lossy().into_owned()
}

fn game_file(dir: &Path) -> String {
    dir.join("global.ini").to_string_lossy().into_owned()
}

fn note_file(dir: &Path) -> String {
    dir.join("injektion-urtext.json").to_string_lossy().into_owned()
}

/// The number of the writing the text file stands at (`; Stand <n>` in its first line).
fn file_stand(dir: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(game_file(dir)).ok()?;
    text.lines().next()?.strip_prefix("; Stand ")?.parse().ok()
}

/// The number of the writing the note belongs to.
fn note_stand(dir: &Path) -> Option<u64> {
    match jsonio::read_json(&note_file(dir)).ok()?.get("stand")? {
        Value::Int(number) => number.parse().ok(),
        _ => None,
    }
}

fn write_pair(dir: &Path, stand: u64, pause: Duration) {
    let text = format!("; Stand {stand}\nMenu_Key=Text {stand}\n");
    injection::write_game_file(&game_file(dir), text.as_bytes()).expect("text file written");
    // Between the file and its note the pair does not fit together: another writer must not look in now.
    std::thread::sleep(pause);
    let note = Value::Object(vec![("stand".to_string(), Value::int(i64::try_from(stand).unwrap_or(0)))]);
    jsonio::save_json(&note_file(dir), &note, 0, false).expect("note written");
}

fn spawn(role: &str, dir: &Path, extra: &[(&str, String)]) -> Child {
    let mut command = Command::new(std::env::current_exe().expect("own program"));
    command
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env(ROLE, role)
        .env(DIR, dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in extra {
        command.env(key, value);
    }
    command.spawn().expect("child starts")
}

fn wait_for(path: &Path, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn finish(mut child: Child) {
    let status = child.wait().expect("child ends");
    assert!(status.success(), "child failed: {status:?}");
}

/// The part of the other process; a normal run of the test does nothing here.
#[test]
fn child_entry() {
    let Ok(role) = std::env::var(ROLE) else { return };
    let dir = PathBuf::from(std::env::var(DIR).expect("folder"));
    if std::env::var(SHARE).as_deref() != Ok("0") {
        assert!(injection::share_game_files_between_processes(&lock_file(&dir)), "lock file opens");
    }
    match role.as_str() {
        // Read the pair, see that it fits, write the next writing, as the background and the steps of the pages
        // do with a text file and its note.
        "writer" => {
            let name = std::env::var(NAME).expect("name");
            let count: usize = std::env::var(COUNT).expect("count").parse().expect("number");
            std::fs::write(dir.join(format!("bereit-{name}.txt")), "ja").expect("ready");
            assert!(wait_for(&dir.join("los.txt"), Duration::from_secs(120)), "the start comes");
            let mut torn = 0;
            for _ in 0..count {
                let _one_writer = injection::game_file_lock();
                let (file, note) = (file_stand(&dir), note_stand(&dir));
                if file != note {
                    torn += 1;
                }
                let next = file.unwrap_or(0).max(note.unwrap_or(0)) + 1;
                std::thread::sleep(Duration::from_millis(2));
                write_pair(&dir, next, Duration::from_millis(4));
            }
            std::fs::write(dir.join(format!("zerrissen-{name}.txt")), torn.to_string()).expect("result");
        }
        // Write the text file of writing 1, stop between the file and its note until told, then write the note.
        "halfway" => {
            let _one_writer = injection::game_file_lock();
            injection::write_game_file(&game_file(&dir), b"; Stand 1\nMenu_Key=Text 1\n").expect("text file written");
            std::fs::write(dir.join("mitte.txt"), "ja").expect("mark");
            assert!(wait_for(&dir.join("weiter.txt"), Duration::from_secs(120)), "told to go on");
            let note = Value::Object(vec![("stand".to_string(), Value::int(1))]);
            jsonio::save_json(&note_file(&dir), &note, 0, false).expect("note written");
        }
        // Read the pair under the lock and say what was seen: `<file> <note>`.
        "reader" => {
            let _one_writer = injection::game_file_lock();
            let (file, note) = (file_stand(&dir), note_stand(&dir));
            let seen = format!("{} {}", file.unwrap_or(0), note.unwrap_or(0));
            std::fs::write(dir.join("gesehen.txt"), seen).expect("result");
        }
        // Hold the lock until told, so the other process has to wait.
        "holder" => {
            let hold: u64 = std::env::var(HOLD_MS).expect("hold").parse().expect("number");
            let _one_writer = injection::game_file_lock();
            std::fs::write(dir.join("haelt.txt"), "ja").expect("mark");
            std::thread::sleep(Duration::from_millis(hold));
        }
        // Write the notes into the text file of the game again and again, in turn in two languages, as the overlay
        // and the main window do after a switch of the language; every report of a lock waited for in vain is kept.
        "notes" => {
            let name = std::env::var(NAME).expect("name");
            let count: usize = std::env::var(COUNT).expect("count").parse().expect("number");
            let paths = notes_paths(&dir);
            assert!(paths.share_saves(), "both locks shared");
            injection::set_game_files_wait(NOTES_WAIT);
            let reports = dir.join(format!("meldungen-{name}.txt"));
            REPORTS_TO.lock().unwrap_or_else(|e| e.into_inner()).replace(reports.clone());
            versekit_core::errors::set_sink(|label, text| {
                if label.ends_with(".sperre")
                    && let Some(file) = REPORTS_TO.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
                {
                    let old = std::fs::read_to_string(file).unwrap_or_default();
                    let _ = std::fs::write(file, format!("{old}{label}: {text}\n"));
                }
            });
            std::fs::write(&reports, "").expect("reports");
            std::fs::write(dir.join(format!("bereit-{name}.txt")), "ja").expect("ready");
            assert!(wait_for(&dir.join("los.txt"), Duration::from_secs(120)), "the start comes");
            // Made once: between two writings nothing else happens, as in a process that writes again and again.
            let (ini, inputs) = (notes_ini(&dir), notes_inputs());
            for round in 0..count {
                let language = if round % 2 == 0 { "german_(germany)" } else { "english" };
                let outcome = injection::apply_texts(&paths, &ini, language, &inputs, false);
                assert!(matches!(outcome, injection::Outcome::Done(_)), "written: {outcome:?}");
            }
        }
        other => panic!("unknown role {other}"),
    }
}

/// How long a process of [`two_processes_writing_the_notes_at_once_never_wait_in_vain`] waits for the other one:
/// far below the wait of the program. On a busy machine one writing of a debug build may take as long
/// (`held … for 5 s`): that is no starving, see [`starved`].
const NOTES_WAIT: Duration = Duration::from_secs(5);

/// Whether a report of a lock waited for in vain tells of starving: the holder had taken the lock only a moment
/// before (`held … for 0 s`), so the waiting one missed its turn again and again. A holder that held it the whole
/// wait in one writing (a slow machine) is no starving.
fn starved(report: &str) -> bool {
    let held = report.split(" for ").nth(1).and_then(|rest| rest.split(' ').next()).and_then(|s| s.parse::<u64>().ok());
    held.is_none_or(|seconds| seconds + 1 < NOTES_WAIT.as_secs())
}

/// Where the child process keeps the reports of the locks.
static REPORTS_TO: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The data folder and the game folder of the notes test, both in `dir`, alike for both processes.
fn notes_paths(dir: &Path) -> versekit_core::paths::Paths {
    use versekit_core::paths::{Host, Paths};
    let root = dir.to_string_lossy().into_owned();
    let (data, game, config) = (format!("{root}/data"), format!("{root}/StarCitizen/LIVE"), format!("{root}/config"));
    // The home folder of every system (Windows reads USERPROFILE, not HOME) and the configuration folder, where the
    // lock files of the shared locks lie: all in the throwaway folder. Without them the home stays a bare "~", taken
    // as a folder below the folder the check runs in.
    Paths::new(Host::new(
        versekit_core::pypath::Flavor::native(),
        &[
            ("HOME", root.as_str()),
            ("USERPROFILE", root.as_str()),
            ("APPDATA", config.as_str()),
            ("XDG_CONFIG_HOME", config.as_str()),
            ("VERSEKIT_HOME", data.as_str()),
            ("SC_INSTALL_DIR", game.as_str()),
        ],
        &root,
    ))
}

fn notes_ini(dir: &Path) -> String {
    dir.join("StarCitizen/LIVE/data/Localization/german_(germany)/global.ini").to_string_lossy().into_owned()
}

fn notes_inputs() -> injection::Inputs {
    let missions = jsonio::loads(
        r#"{"Bounty_title_001": {"bp": ["Calico Helmet"], "sicher": true, "rep": 800,
            "titel_key": "Bounty_title_001", "text_key": "Bounty_desc_001"}}"#,
    )
    .expect("contracts")
    .as_object()
    .expect("object")
    .clone();
    injection::Inputs { missions, version: "5.6.5".into(), ..injection::Inputs::default() }
}

/// A text file of the size of the game's (some 80 000 lines, over 10 MB) in `dir`, with the data folder beside it.
fn notes_fixture(dir: &Path) {
    let mut text = String::from("\u{feff}Frontend_PU_Version=Star Citizen 4.4\nBounty_title_001=Kopfgeld\n");
    text.push_str("Bounty_desc_001=Fang ihn.\n");
    for line in 0..80_000 {
        text.push_str(&format!(
            "item_Desc_{line:05}=Hersteller: X\\nGröße: 1\\nGütegrad: A\\nKlasse: Militär\\n{}\n",
            "x".repeat(100)
        ));
    }
    let ini = notes_ini(dir);
    std::fs::create_dir_all(Path::new(&ini).parent().expect("folder")).expect("game folder");
    std::fs::create_dir_all(dir.join("data")).expect("data folder");
    std::fs::write(&ini, text).expect("text file");
}

/// The overlay and the main window write the notes at once after a switch of the language, each again and again:
/// each waits for the other only while that one writes, and no `injection.sperre` (nor `jsonio.sperre`) is reported.
/// Without the lock of the turn the one that let go took the lock back each time before the waiting one asked
/// again: that one waited the whole wait in vain (`held … for 0 s`) and then wrote without the lock.
#[test]
fn two_processes_writing_the_notes_at_once_never_wait_in_vain() {
    let _serial = serial();
    let dir = Dir::new("spielsperre-hinweise");
    notes_fixture(dir.path());
    let children: Vec<Child> = ["overlay", "hauptfenster"]
        .iter()
        .map(|name| spawn("notes", dir.path(), &[(NAME, (*name).to_string()), (COUNT, "6".to_string())]))
        .collect();
    for name in ["overlay", "hauptfenster"] {
        assert!(wait_for(&dir.path().join(format!("bereit-{name}.txt")), Duration::from_secs(120)), "{name} ready");
    }
    std::fs::write(dir.path().join("los.txt"), "los").expect("start");
    children.into_iter().for_each(finish);
    for name in ["overlay", "hauptfenster"] {
        let reports = std::fs::read_to_string(dir.path().join(format!("meldungen-{name}.txt"))).expect("reports");
        let starving: Vec<&str> = reports.lines().filter(|report| starved(report)).collect();
        assert!(starving.is_empty(), "{name} waited in vain: {starving:?}");
    }
    let written = std::fs::read_to_string(notes_ini(dir.path())).expect("text file");
    assert_eq!(written.matches("VerseKit v5.6.5").count(), 1, "one menu line, nothing stacked");
}

/// Two writers at once, each [`WRITINGS`] times: how many pairs that did not fit they saw, and the stand at the end.
fn race(share: bool) -> (usize, Option<u64>, Option<u64>) {
    let dir = Dir::new("spielsperre");
    write_pair(dir.path(), 0, Duration::ZERO);
    let share = if share { "1" } else { "0" };
    let children: Vec<Child> = ["a", "b"]
        .iter()
        .map(|name| {
            spawn(
                "writer",
                dir.path(),
                &[(NAME, (*name).to_string()), (COUNT, WRITINGS.to_string()), (SHARE, share.to_string())],
            )
        })
        .collect();
    for name in ["a", "b"] {
        assert!(wait_for(&dir.path().join(format!("bereit-{name}.txt")), Duration::from_secs(120)), "{name} ready");
    }
    std::fs::write(dir.path().join("los.txt"), "los").expect("start");
    children.into_iter().for_each(finish);
    let torn: usize = ["a", "b"]
        .iter()
        .map(|name| {
            std::fs::read_to_string(dir.path().join(format!("zerrissen-{name}.txt")))
                .expect("result")
                .trim()
                .parse::<usize>()
                .expect("number")
        })
        .sum();
    (torn, file_stand(dir.path()), note_stand(dir.path()))
}

#[test]
fn under_the_shared_lock_two_processes_never_see_a_torn_pair_and_lose_no_writing() {
    let _serial = serial();
    let (torn, file, note) = race(true);
    assert_eq!(torn, 0, "no writer saw the file of one writing with the note of another");
    let all = u64::try_from(2 * WRITINGS).unwrap_or(0);
    assert_eq!((file, note), (Some(all), Some(all)), "every writing of both counts");
}

/// One writer stopped between the text file and its note, a reader in another process meanwhile: what the reader
/// saw while the writer stood still (`None`: nothing within `wait`), and what it saw in the end.
fn reader_beside_a_half_writing(share: bool, wait: Duration) -> (Option<String>, String) {
    let dir = Dir::new("spielsperre-halb");
    write_pair(dir.path(), 0, Duration::ZERO);
    let share = [(SHARE, if share { "1" } else { "0" }.to_string())];
    let writer = spawn("halfway", dir.path(), &share);
    assert!(wait_for(&dir.path().join("mitte.txt"), Duration::from_secs(120)), "the writer stands in the middle");
    let reader = spawn("reader", dir.path(), &share);
    let seen = dir.path().join("gesehen.txt");
    let early = wait_for(&seen, wait).then(|| std::fs::read_to_string(&seen).expect("result"));
    std::fs::write(dir.path().join("weiter.txt"), "los").expect("go on");
    finish(writer);
    finish(reader);
    (early, std::fs::read_to_string(&seen).expect("result"))
}

/// The counter-check of the test above, without a race on the clock: without the shared lock a reader in another
/// process looks in between the file and its note and sees the pair torn.
#[test]
fn without_the_shared_lock_a_reader_sees_the_pair_torn() {
    let _serial = serial();
    let (early, _) = reader_beside_a_half_writing(false, Duration::from_secs(120));
    assert_eq!(early.as_deref(), Some("1 0"), "the file of writing 1 with the note of writing 0");
}

/// The same with the shared lock: the reader waits for the whole writing and then sees it whole.
#[test]
fn with_the_shared_lock_a_reader_waits_for_the_whole_writing() {
    let _serial = serial();
    let (early, last) = reader_beside_a_half_writing(true, Duration::from_secs(2));
    assert_eq!(early, None, "nothing is read while the writer stands in the middle");
    assert_eq!(last, "1 1");
}

#[test]
fn a_writer_waits_for_the_other_process_and_then_goes_on() {
    let _serial = serial();
    let dir = Dir::new("spielsperre-warten");
    assert!(injection::share_game_files_between_processes(&lock_file(dir.path())), "lock file opens");
    let holder = spawn("holder", dir.path(), &[(HOLD_MS, "600".to_string())]);
    assert!(wait_for(&dir.path().join("haelt.txt"), Duration::from_secs(60)), "the other process holds the lock");
    let start = Instant::now();
    {
        let _one_writer = injection::game_file_lock();
        assert!(start.elapsed() >= Duration::from_millis(200), "it waited for the other process");
    }
    finish(holder);
    // A holder that is gone leaves nothing behind: the lock is free at once.
    let start = Instant::now();
    drop(injection::game_file_lock());
    assert!(start.elapsed() < Duration::from_secs(1), "no lock is left behind");
}

#[test]
fn a_stuck_process_holds_the_writer_only_as_long_as_the_wait() {
    let _serial = serial();
    let dir = Dir::new("spielsperre-grenze");
    assert!(injection::share_game_files_between_processes(&lock_file(dir.path())), "lock file opens");
    injection::set_game_files_wait(Duration::from_millis(300));
    let holder = spawn("holder", dir.path(), &[(HOLD_MS, "5000".to_string())]);
    assert!(wait_for(&dir.path().join("haelt.txt"), Duration::from_secs(60)), "the other process holds the lock");
    let start = Instant::now();
    drop(injection::game_file_lock());
    let waited = start.elapsed();
    injection::set_game_files_wait(injection::GAME_FILES_WAIT);
    assert!(waited >= Duration::from_millis(250) && waited < Duration::from_secs(4), "waited {waited:?}");
    finish(holder);
}

/// What the error log received during the test of the holder.
static REPORTED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// A writer that waited in vain says who held the game files: the process, for how long and the place in the source
/// that took the lock. Once the holder lets go its note is empty again.
#[test]
fn a_writer_that_waited_in_vain_names_the_holder() {
    let _serial = serial();
    let dir = Dir::new("spielsperre-halter");
    let lock = lock_file(dir.path());
    assert!(injection::share_game_files_between_processes(&lock), "lock file opens");
    versekit_core::errors::set_sink(|label, text| {
        REPORTED.lock().unwrap_or_else(|e| e.into_inner()).push((label.to_string(), text.to_string()));
    });
    injection::set_game_files_wait(Duration::from_millis(300));
    let holder = spawn("holder", dir.path(), &[(HOLD_MS, "3000".to_string())]);
    let process = holder.id();
    assert!(wait_for(&dir.path().join("haelt.txt"), Duration::from_secs(60)), "the other process holds the lock");
    drop(injection::game_file_lock());
    injection::set_game_files_wait(injection::GAME_FILES_WAIT);
    let said: Vec<String> = REPORTED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(label, _)| label == "injection.sperre")
        .map(|(_, text)| text.clone())
        .collect();
    assert_eq!(said.len(), 1, "one report: {said:?}");
    assert!(said[0].contains(&format!("(process {process})")), "the holder is named: {}", said[0]);
    assert!(said[0].contains("tests/game_lock.rs:"), "with the place that took the lock: {}", said[0]);
    assert!(!said[0].contains(&dir.path().to_string_lossy().into_owned()), "no folder of the player: {}", said[0]);
    finish(holder);
    let note = std::fs::read_to_string(injection::holder_file(&lock)).unwrap_or_default();
    assert_eq!(note, "", "the note is empty once the holder let go");
}

#[test]
fn the_holder_note_reads_as_role_process_time_and_place() {
    assert_eq!(
        injection::holder_text("overlay 4711 100 crates/versekit-gamedata/src/game_text.rs:881", 235),
        "held by overlay (process 4711) for 135 s, taken at crates/versekit-gamedata/src/game_text.rs:881"
    );
    assert_eq!(injection::holder_text("", 235), "holder unknown", "a holder that has not written its note yet");
    assert_eq!(injection::holder_file("a/sperre-spiel-x.lock"), "a/sperre-spiel-x-halter.lock");
}
