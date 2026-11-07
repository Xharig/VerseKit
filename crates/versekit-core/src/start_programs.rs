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

//! Start programs (`start_programs.py`): further programs that start with the game.
//!
//! Every entry says **when** it starts: `launcher` together with the RSI Launcher (the button
//! "RSI Launcher starten"), `spiel` as soon as Star Citizen runs (the watch for the end of the game
//! looks every twenty seconds), `ersetzt` **instead of** the RSI Launcher (the button then opens this
//! program).
//!
//! ⚠⚠ Exactly what the player enters is run, nothing else: no preset entries, no command from the
//! network. The page says so (own risk).
//!
//! ⚠⚠ A check never starts anything: the start goes through [`Spawner`], which a check replaces by a
//! recorder.

use std::sync::Mutex;
use std::time::Duration;

use crate::game_start::{Running, Spawner, shlex_split};
use crate::jsonio::{Map, Value};
use crate::paths::Paths;
use crate::pypath::Flavor;

/// The main switch (on by default: the list is empty by default).
pub const SETTING_ON: &str = "startprogramme_an";
/// The list of the entries.
pub const SETTING_LIST: &str = "startprogramme";

/// When an entry starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// Together with the RSI Launcher.
    Launcher,
    /// As soon as the game runs.
    Game,
    /// Instead of the RSI Launcher.
    Replaces,
}

impl When {
    /// The word as it is stored.
    pub fn stored(self) -> &'static str {
        match self {
            When::Launcher => "launcher",
            When::Game => "spiel",
            When::Replaces => "ersetzt",
        }
    }

    /// An unknown word counts as `launcher`.
    pub fn of(word: &str) -> When {
        match word {
            "spiel" => When::Game,
            "ersetzt" => When::Replaces,
            _ => When::Launcher,
        }
    }
}

/// One start program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Switched on (`an`).
    pub on: bool,
    pub name: String,
    /// The file (`datei`).
    pub file: String,
    /// The arguments as one text (`argumente`).
    pub args: String,
    pub when: When,
    /// Seconds to wait before the start (`warten`).
    pub wait: i64,
    /// End it again when the game ends (`beenden`).
    pub end: bool,
}

impl Default for Entry {
    fn default() -> Entry {
        Entry {
            on: true,
            name: String::new(),
            file: String::new(),
            args: String::new(),
            when: When::Launcher,
            wait: 0,
            end: false,
        }
    }
}

/// Python's truth of a stored value.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => n.trim_start_matches('-').chars().any(|c| c != '0'),
        Value::Float(x) => *x != 0.0,
        Value::Str(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// `int(value or 0)` for a number, a text or nothing; anything else is 0.
fn whole(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Int(n)) => n.parse().unwrap_or(0),
        Some(Value::Float(x)) if x.is_finite() => *x as i64,
        Some(Value::Str(s)) => s.trim().parse().unwrap_or(0),
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    }
}

fn text_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => crate::jsonio::dumps(other, None, false),
    }
}

/// The main switch.
pub fn enabled(paths: &Paths) -> bool {
    paths.setting_bool(SETTING_ON, true)
}

/// All entries, each complete (missing fields with their defaults). Not text in the settings (a list)
/// is read from the whole settings, not from `Paths::setting`, which gives text only.
pub fn entries(paths: &Paths) -> Vec<Entry> {
    let settings = paths.settings();
    let Some(Value::Array(items)) = settings.iter().find(|(k, _)| k == SETTING_LIST).map(|(_, v)| v) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let Value::Object(_) = item else { return None };
            let mut entry = Entry::default();
            if let Some(v) = item.get("an") {
                entry.on = truthy(v);
            }
            entry.name = text_of(item.get("name"));
            entry.file = text_of(item.get("datei"));
            entry.args = text_of(item.get("argumente"));
            if let Some(v) = item.get("wann") {
                entry.when = When::of(v.as_str().unwrap_or(""));
            }
            entry.wait = whole(item.get("warten"));
            if let Some(v) = item.get("beenden") {
                entry.end = truthy(v);
            }
            Some(entry)
        })
        .collect()
}

/// Stores the list. True if it was written.
pub fn save(paths: &Paths, items: &[Entry]) -> bool {
    let list = items
        .iter()
        .map(|e| {
            let fields: Map = vec![
                ("an".into(), Value::Bool(e.on)),
                ("name".into(), Value::str(e.name.as_str())),
                ("datei".into(), Value::str(e.file.as_str())),
                ("argumente".into(), Value::str(e.args.as_str())),
                ("wann".into(), Value::str(e.when.stored())),
                ("warten".into(), Value::int(e.wait)),
                ("beenden".into(), Value::Bool(e.end)),
            ];
            Value::Object(fields)
        })
        .collect();
    paths.set_setting(SETTING_LIST, Value::Array(list))
}

/// Settings without a switch that go to the system unchanged: from a backup the value of before holds
/// (the start command for Star Citizen, the own address of the translation).
pub const KEEP_PREVIOUS: [&str; 2] = [crate::game_start::STARTER_SETTING, crate::game_ini::SETTING_CUSTOM];

/// Python's `str(value)` of a value that is true (the truth values print as `True` there).
fn py_text(value: &Value) -> String {
    match value {
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        other => text_of(Some(other)),
    }
}

/// What an entry runs: file, arguments, moment (`_command_of` of the backup), each cut at the blanks.
fn command_of(item: &Value) -> (String, String, When) {
    let text = |key: &str| match item.get(key) {
        Some(value) if truthy(value) => py_text(value).trim().to_string(),
        _ => String::new(),
    };
    let when = item.get("wann").and_then(Value::as_str).map_or(When::Launcher, When::of);
    (text("datei"), text("argumente"), when)
}

/// One line, cut to `limit` characters with an ellipsis.
fn one_line(value: Option<&Value>, limit: usize) -> String {
    let text = match value {
        Some(value) if truthy(value) => py_text(value),
        _ => String::new(),
    };
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= limit {
        line
    } else {
        let mut cut: String = line.chars().take(limit - 1).collect();
        cut.push('…');
        cut
    }
}

/// Name and file of an entry for the note after a restore.
fn shown_name(item: &Value) -> String {
    let name = one_line(item.get("name"), 40);
    let file = {
        let line = one_line(item.get("datei"), 80);
        if line.is_empty() { "?".to_string() } else { line }
    };
    if name.is_empty() { file } else { format!("{name} ({file})") }
}

/// The settings of a backup made safe (`_safe_settings` of the backup, the part about commands).
///
/// ⚠⚠ A backup may come from somebody else, and these values go to the system unchanged. So an entry
/// stays on only if **exactly the same command** (file, arguments, moment) was on in `before`; every
/// other arrives switched off and stands in the list, to be switched on by someone who has seen the
/// command. The settings of `KEEP_PREVIOUS` have no switch: where they differ, `before` holds.
/// Returns what was switched off or not taken over: the names of the entries, then the keys.
pub fn safe_import(incoming: &mut Map, before: &Map) -> Vec<String> {
    let mut turned_off = Vec::new();
    let known = |key: &str| before.iter().find(|(k, _)| k == key).map(|(_, v)| v);
    let own: Vec<(String, String, When)> = match known(SETTING_LIST) {
        Some(Value::Array(items)) => items
            .iter()
            .filter(|item| matches!(item, Value::Object(_)))
            .filter(|item| item.get("an").is_none_or(truthy))
            .map(command_of)
            .collect(),
        _ => Vec::new(),
    };
    if let Some(slot) = incoming.iter().position(|(k, _)| k == SETTING_LIST) {
        match &incoming[slot].1 {
            Value::Array(items) => {
                let mut kept = Vec::new();
                for item in items {
                    let Value::Object(fields) = item else { continue };
                    let mut fields = fields.clone();
                    if !own.contains(&command_of(item)) {
                        if item.get("an").is_none_or(truthy) {
                            turned_off.push(shown_name(item));
                        }
                        crate::jsonio::map_set(&mut fields, "an", Value::Bool(false));
                    }
                    kept.push(Value::Object(fields));
                }
                incoming[slot].1 = Value::Array(kept);
            }
            Value::Null => {}
            _ => {
                incoming.remove(slot);
            }
        }
    }
    for key in KEEP_PREVIOUS {
        let restored = incoming.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let previous = known(key);
        let (restored_on, previous_on) = (restored.is_some_and(truthy), previous.is_some_and(truthy));
        if crate::jsonio::py_eq(restored, previous) || (!restored_on && !previous_on) {
            continue;
        }
        match previous.filter(|p| truthy(p)) {
            Some(previous) => crate::jsonio::map_set(incoming, key, previous.clone()),
            None => incoming.retain(|(k, _)| k != key),
        }
        if restored_on {
            turned_off.push(key.to_string());
        }
    }
    turned_off
}

/// The entries that are on, with a file, for this moment (nothing while the main switch is off).
fn active(paths: &Paths, when: When) -> Vec<Entry> {
    if !enabled(paths) {
        return Vec::new();
    }
    entries(paths).into_iter().filter(|e| e.on && e.when == when && !e.file.trim().is_empty()).collect()
}

/// The command for the file and its arguments (split as a shell does; an argument text that cannot
/// be split is cut at the blanks).
pub fn command(entry: &Entry, flavor: Flavor) -> Vec<String> {
    let mut command = vec![entry.file.trim().to_string()];
    let args = entry.args.trim();
    if !args.is_empty() {
        match shlex_split(args, flavor != Flavor::Windows) {
            Some(parts) => command.extend(parts),
            None => command.extend(args.split_whitespace().map(str::to_string)),
        }
    }
    command
}

/// What was started in this run and is to be ended with the game: the handles.
static RUNNING: Mutex<Vec<Box<dyn Running>>> = Mutex::new(Vec::new());

/// Why an entry did not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchError {
    /// The entry has no file yet.
    NoFile,
    /// The file is not there (its path).
    Missing(String),
    /// The system refused to start it (its reason).
    Failed(String),
}

impl LaunchError {
    /// The reason in the language of the program, with what to do about it.
    pub fn reason(&self) -> String {
        use crate::language::{Arg, t, tf};
        match self {
            LaunchError::NoFile => t("s_pg_grund_leer"),
            LaunchError::Missing(path) => tf("s_pg_grund_weg", &[("0", Arg::from(path.as_str()))]),
            LaunchError::Failed(reason) => reason.clone(),
        }
    }
}

/// How often the game ended in this run: a start that waited meanwhile asks (see [`launch`]).
static GAME_ENDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Starts an entry after its time to wait (`delay`), in the folder of its file. `Err` when it has no
/// file, the file is not there, or (without a time to wait) the system refused. With a time to wait the
/// answer comes at once and the start follows in the background; a refusal there is only written to
/// the error report.
///
/// ⚠ An entry that waits, belongs to the game (`spiel`) or asked to be ended with it, does not start
/// after the wait if the game ended meanwhile: it would run on with nobody left to end it.
pub fn launch(
    entry: &Entry,
    spawner: std::sync::Arc<dyn Spawner>,
    flavor: Flavor,
    delay: bool,
) -> Result<(), LaunchError> {
    use std::sync::atomic::Ordering;
    let path = entry.file.trim().to_string();
    if path.is_empty() {
        return Err(LaunchError::NoFile);
    }
    if !std::path::Path::new(&path).exists() {
        return Err(LaunchError::Missing(path));
    }
    let folder = flavor_dirname(&path, flavor);
    let (command, end) = (command(entry, flavor), entry.end);
    let go = move || -> Result<(), LaunchError> {
        match spawner.spawn_program(&command, folder.as_deref()) {
            Ok(process) => {
                if end {
                    RUNNING.lock().unwrap_or_else(|e| e.into_inner()).push(process);
                }
                Ok(())
            }
            Err(reason) => {
                crate::errors::record("start_programs.launch", &format!("{path}: {reason}"));
                Err(LaunchError::Failed(reason))
            }
        }
    };
    let wait = if delay { entry.wait.max(0) } else { 0 };
    if wait > 0 {
        let (generation, tied_to_game) = (GAME_ENDS.load(Ordering::SeqCst), entry.when == When::Game || end);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(wait.unsigned_abs()));
            if tied_to_game && GAME_ENDS.load(Ordering::SeqCst) != generation {
                return;
            }
            let _ = go();
        });
        Ok(())
    } else {
        go()
    }
}

/// The folder of a file, `None` when it has none.
fn flavor_dirname(path: &str, flavor: Flavor) -> Option<String> {
    let folder = flavor.dirname(path);
    (!folder.is_empty()).then_some(folder)
}

/// The entry that replaces the RSI Launcher, if there is one.
pub fn replacement(paths: &Paths) -> Option<Entry> {
    active(paths, When::Replaces).into_iter().next()
}

/// With the RSI Launcher: every entry with `launcher`.
pub fn on_launcher(paths: &Paths, spawner: &std::sync::Arc<dyn Spawner>) {
    for entry in active(paths, When::Launcher) {
        let _ = launch(&entry, spawner.clone(), paths.host.flavor, true);
    }
}

/// The game runs: every entry with `spiel`.
pub fn on_game_started(paths: &Paths, spawner: &std::sync::Arc<dyn Spawner>) {
    for entry in active(paths, When::Game) {
        let _ = launch(&entry, spawner.clone(), paths.host.flavor, true);
    }
}

/// The game ended: what was started with the wish to end it again ends now.
pub fn on_game_ended() {
    GAME_ENDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    // Taken out first: ending a program may take a moment, and nobody else waits for the list meanwhile.
    let taken = std::mem::take(&mut *RUNNING.lock().unwrap_or_else(|e| e.into_inner()));
    for mut process in taken {
        if process.is_running() {
            process.terminate();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::game_start::Recorder;
    use crate::paths::Host;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-startprog-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn paths_in(home: &str) -> Paths {
        Paths::new(Host::new(Flavor::Posix, &[("SC_BP_HOME", home), ("HOME", home)], home))
    }

    fn file_in(home: &str, name: &str) -> String {
        let path = format!("{home}/{name}");
        std::fs::write(&path, "x").expect("file");
        path
    }

    fn entry(file: &str, when: When) -> Entry {
        Entry { name: "Tool".into(), file: file.into(), when, ..Entry::default() }
    }

    #[test]
    fn the_list_is_stored_and_read_back_with_the_defaults_for_what_is_missing() {
        let home = scratch("liste");
        let paths = paths_in(&home);
        assert!(entries(&paths).is_empty());
        let items = vec![
            Entry {
                on: false,
                name: "A".into(),
                file: "/x/a".into(),
                args: "-v".into(),
                when: When::Game,
                wait: 5,
                end: true,
            },
            entry("/x/b", When::Replaces),
        ];
        assert!(save(&paths, &items));
        assert_eq!(entries(&paths), items);
        // A hand-edited list: missing fields take their defaults, an unknown time counts as the launcher,
        // what is no entry is passed over, a text for the seconds is read as a number.
        let text = r#"{"startprogramme": [{"datei": "/x/c", "wann": "irgendwann", "warten": "7"}, 3, {"an": 0, "name": "D"}]}"#;
        std::fs::write(paths.settings_file(), text).expect("settings");
        paths.forget_settings();
        let read = entries(&paths);
        assert_eq!(read.len(), 2);
        assert_eq!((read[0].file.as_str(), read[0].when, read[0].wait, read[0].on), ("/x/c", When::Launcher, 7, true));
        assert_eq!((read[1].name.as_str(), read[1].on), ("D", false));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn only_entries_that_are_on_with_a_file_for_this_moment_count_and_the_main_switch_stops_all() {
        let home = scratch("aktiv");
        let paths = paths_in(&home);
        let file = file_in(&home, "tool.exe");
        let items = vec![
            entry(&file, When::Launcher),
            Entry { on: false, ..entry(&file, When::Launcher) },
            entry("   ", When::Launcher),
            entry(&file, When::Game),
            entry(&file, When::Replaces),
        ];
        assert!(save(&paths, &items));
        assert_eq!(active(&paths, When::Launcher).len(), 1);
        assert_eq!(active(&paths, When::Game).len(), 1);
        assert_eq!(replacement(&paths).map(|e| e.when), Some(When::Replaces));
        assert!(paths.set_setting(SETTING_ON, Value::Bool(false)));
        assert!(
            active(&paths, When::Launcher).is_empty() && replacement(&paths).is_none(),
            "the main switch stops all"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_command_is_the_file_and_the_arguments_split_as_a_shell_does() {
        let item = Entry { file: " /x/tool ".into(), args: "-a 'b c' d".into(), ..Entry::default() };
        assert_eq!(command(&item, Flavor::Posix), ["/x/tool", "-a", "b c", "d"]);
        // An argument text that cannot be split is cut at the blanks.
        let broken = Entry { file: "/x/tool".into(), args: "-a 'b c".into(), ..Entry::default() };
        assert_eq!(command(&broken, Flavor::Posix), ["/x/tool", "-a", "'b", "c"]);
        assert_eq!(command(&Entry { file: "/x/tool".into(), ..Entry::default() }, Flavor::Posix), ["/x/tool"]);
    }

    #[test]
    fn a_missing_file_is_a_reason_and_nothing_started_and_a_present_one_starts_in_its_folder() {
        let home = scratch("start");
        let recorder = Arc::new(Recorder::default());
        let spawner: Arc<dyn Spawner> = recorder.clone();
        assert_eq!(launch(&entry("", When::Launcher), spawner.clone(), Flavor::Posix, false), Err(LaunchError::NoFile));
        let gone = format!("{home}/gibt-es-nicht");
        assert_eq!(
            launch(&entry(&gone, When::Launcher), spawner.clone(), Flavor::Posix, false),
            Err(LaunchError::Missing(gone))
        );
        assert!(recorder.program_calls().is_empty());
        let file = file_in(&home, "tool.exe");
        let item = Entry { args: "--go".into(), ..entry(&file, When::Launcher) };
        assert_eq!(launch(&item, spawner, Flavor::Posix, false), Ok(()));
        let calls = recorder.program_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].command, [file.as_str(), "--go"]);
        assert_eq!(calls[0].cwd.as_deref(), Some(home.as_str()), "the folder of the file");
        let _ = std::fs::remove_dir_all(home);
    }

    /// A system that refuses every program.
    struct Refuses;

    impl Spawner for Refuses {
        fn spawn(&self, _: &[String], _: &[(String, String)]) -> Result<(), String> {
            Err("refused".to_string())
        }

        fn spawn_program(&self, _: &[String], _: Option<&str>) -> Result<Box<dyn Running>, String> {
            Err("Zugriff verweigert".to_string())
        }
    }

    #[test]
    fn a_start_the_system_refuses_is_an_error_and_not_a_started_program() {
        let home = scratch("verweigert");
        let file = file_in(&home, "tool.exe");
        let spawner: Arc<dyn Spawner> = Arc::new(Refuses);
        let error = launch(&entry(&file, When::Launcher), spawner, Flavor::Posix, false).expect_err("refused");
        assert_eq!(error, LaunchError::Failed("Zugriff verweigert".into()));
        assert_eq!(error.reason(), "Zugriff verweigert");
        // The reasons of a missing file are words with what to do, no dash and no bare path.
        for (error, part) in [(LaunchError::NoFile, "Datei"), (LaunchError::Missing("/x/y".into()), "/x/y")] {
            let text = error.reason();
            assert!(text.contains(part) && !text.contains('—') && text.len() > 12, "{text}");
        }
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn what_waited_for_the_game_does_not_start_when_the_game_ended_meanwhile() {
        let home = scratch("wartet-spielende");
        let recorder = Arc::new(Recorder::default());
        let spawner: Arc<dyn Spawner> = recorder.clone();
        let file = file_in(&home, "tool.exe");
        let tied = Entry { wait: 1, ..entry(&file, When::Game) };
        let free = Entry { wait: 1, ..entry(&file, When::Launcher) };
        assert_eq!(launch(&tied, spawner.clone(), Flavor::Posix, true), Ok(()));
        assert_eq!(launch(&free, spawner, Flavor::Posix, true), Ok(()));
        on_game_ended();
        std::thread::sleep(Duration::from_millis(2200));
        let calls = recorder.program_calls();
        assert_eq!(calls.len(), 1, "only the one that has nothing to do with the game starts: {calls:?}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_time_to_wait_holds_the_start_back_and_the_answer_comes_at_once() {
        let home = scratch("warten");
        let recorder = Arc::new(Recorder::default());
        let spawner: Arc<dyn Spawner> = recorder.clone();
        let file = file_in(&home, "tool.exe");
        let item = Entry { wait: 1, ..entry(&file, When::Launcher) };
        assert_eq!(launch(&item, spawner, Flavor::Posix, true), Ok(()));
        assert!(recorder.program_calls().is_empty(), "not yet");
        for _ in 0..60 {
            if !recorder.program_calls().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(recorder.program_calls().len(), 1, "after the wait it starts");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn with_the_launcher_and_with_the_game_and_the_end_of_the_game_ends_what_asked_for_it() {
        let home = scratch("ende");
        let paths = paths_in(&home);
        let recorder = Arc::new(Recorder::default());
        let spawner: Arc<dyn Spawner> = recorder.clone();
        let file = file_in(&home, "tool.exe");
        let items = vec![Entry { end: true, ..entry(&file, When::Game) }, entry(&file, When::Launcher)];
        assert!(save(&paths, &items));
        on_launcher(&paths, &spawner);
        on_game_started(&paths, &spawner);
        for _ in 0..40 {
            if recorder.program_calls().len() == 2 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(recorder.program_calls().len(), 2, "one with the launcher, one with the game");
        on_game_ended();
        assert_eq!(
            recorder.ended.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "only the one that asked to be ended is ended"
        );
        on_game_ended();
        assert_eq!(recorder.ended.load(std::sync::atomic::Ordering::Relaxed), 1, "and only once");
        let _ = std::fs::remove_dir_all(home);
    }

    fn settings(text: &str) -> Map {
        crate::jsonio::loads(text).expect("json").as_object().expect("object").clone()
    }

    fn on_of(map: &Map, index: usize) -> Value {
        let Some((_, Value::Array(items))) = map.iter().find(|(k, _)| k == SETTING_LIST) else { panic!("no list") };
        items[index].get("an").cloned().unwrap_or(Value::Null)
    }

    #[test]
    fn from_a_backup_only_the_same_command_that_was_on_here_stays_on() {
        let before = settings(
            r#"{"startprogramme": [
                {"an": true, "datei": "/x/ts", "argumente": "-a", "wann": "spiel"},
                {"an": false, "datei": "/x/off", "wann": "launcher"}]}"#,
        );
        let mut incoming = settings(
            r#"{"startprogramme": [
                {"an": true, "name": "Same", "datei": " /x/ts ", "argumente": "-a", "wann": "spiel"},
                {"an": true, "name": "Other file", "datei": "/x/evil", "argumente": "-a", "wann": "spiel"},
                {"an": true, "name": "Other moment", "datei": "/x/ts", "argumente": "-a", "wann": "launcher"},
                {"an": true, "name": "Was off here", "datei": "/x/off", "wann": "launcher"},
                {"an": false, "name": "Off anyway", "datei": "/x/zzz"},
                {"name": "No switch, so on", "datei": "/x/new"},
                "not an entry"]}"#,
        );
        let off = safe_import(&mut incoming, &before);
        let on: Vec<Value> = (0..6).map(|i| on_of(&incoming, i)).collect();
        assert_eq!(
            on,
            [true, false, false, false, false, false].map(Value::Bool),
            "only the command that was on stays on; an entry without a switch counts as on and is switched off"
        );
        assert_eq!(
            off,
            ["Other file (/x/evil)", "Other moment (/x/ts)", "Was off here (/x/off)", "No switch, so on (/x/new)"],
            "and the note names what was switched off (not what was off already)"
        );
        let Some((_, Value::Array(items))) = incoming.iter().find(|(k, _)| k == SETTING_LIST) else { panic!() };
        assert_eq!(items.len(), 6, "what is no entry is dropped, the others stay in the list");
    }

    #[test]
    fn the_start_command_and_the_own_address_of_a_backup_give_way_to_the_ones_here() {
        let before = settings(r#"{"spielstarter": "lutris rungame/x", "eigene_quelle": ""}"#);
        let mut incoming =
            settings(r#"{"spielstarter": "evil --now", "eigene_quelle": "http://example.org/de.ini", "farbe": "rot"}"#);
        let off = safe_import(&mut incoming, &before);
        assert_eq!(
            incoming.iter().find(|(k, _)| k == "spielstarter").map(|(_, v)| v),
            Some(&Value::str("lutris rungame/x"))
        );
        assert!(!incoming.iter().any(|(k, _)| k == "eigene_quelle"), "nothing here, so nothing from the backup");
        assert!(incoming.iter().any(|(k, _)| k == "farbe"), "everything else is left alone");
        assert_eq!(off, ["spielstarter", "eigene_quelle"]);
        // The same value, or none on both sides: nothing to say.
        let mut same = settings(r#"{"spielstarter": "lutris rungame/x"}"#);
        assert!(safe_import(&mut same, &before).is_empty());
        let mut none = settings(r#"{"farbe": "rot"}"#);
        assert!(safe_import(&mut none, &settings("{}")).is_empty());
    }

    #[test]
    fn a_list_that_is_none_is_dropped_and_a_null_stays() {
        let mut broken = settings(r#"{"startprogramme": "rm -rf"}"#);
        assert!(safe_import(&mut broken, &settings("{}")).is_empty());
        assert!(!broken.iter().any(|(k, _)| k == SETTING_LIST));
        let mut nothing = settings(r#"{"startprogramme": null}"#);
        safe_import(&mut nothing, &settings("{}"));
        assert!(nothing.iter().any(|(k, v)| k == SETTING_LIST && *v == Value::Null));
    }

    #[test]
    fn the_name_in_the_note_is_one_line_and_cut() {
        let long = "x".repeat(100);
        let mut incoming = settings(&format!(
            r#"{{"startprogramme": [{{"name": "A\nB   C {long}", "datei": "{long}"}}, {{"datei": ""}}]}}"#
        ));
        let off = safe_import(&mut incoming, &settings("{}"));
        assert_eq!(off.len(), 2);
        assert!(off[0].starts_with("A B C xxx") && off[0].contains('…'), "{}", off[0]);
        assert!(off[0].chars().count() <= 40 + 80 + 3);
        assert_eq!(off[1], "?", "no name and no file");
    }
}
