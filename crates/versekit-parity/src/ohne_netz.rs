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

//! `versekit-parity ohne_netz <Data.p4k> <spielordner> <datenkopie> [--vergleich <global.ini>] [--bestand]
//! [--uex-stand <version>] [--kanal <NAME> <Data.p4k> <spielordner>] [--gegenprobe sperre]`: the program's background with the real game data and every request
//! to the net failing. `--uex-stand` leaves the live build UEX named last in the copy (`spielstand.json`), as it stands
//! after a patch made without the net.
//!
//! A throwaway scenario is laid out in the temporary folder: a copy of the data copy `<datenkopie>` (the folder that
//! holds `VerseKit-Daten`, without its backups; without `--bestand` also without the files of `Intern` that come from
//! the game data or a service of others, as for a player who set VerseKit up but never had them), a copy of the small files of `<spielordner>` (`Game.log`, `build_manifest.id`,
//! `user.cfg`, the `global.ini` files under `data`) and a copy of the launcher's log. `<Data.p4k>` is only read.
//!
//! A child process then runs the watch of the game's texts exactly as the overlay runs it
//! (`versekit_gamedata::texts_watch::watch`, with the real door to the net), until the catalog is built and the notes
//! are written, and after it the reading of the game values as the pages start it. The child reaches the net only
//! through a proxy of this process that writes down every request and answers none (`HTTPS_PROXY` and the others):
//! every request fails as without a connection, and the switch `VERSEKIT_NO_NET` stays unset, so that no code takes the
//! short way a switched-off net offers. `--gegenprobe sperre` gives the child no proxy: the check of the lock must
//! fail then.
//!
//! The result names each of the four parts (catalog, notes in the game text, reputation, values), what came, what
//! stayed empty and every address the child asked for. With `--vergleich` the notes are compared with a `global.ini`
//! written with the net (only read): every contract that carries notes there must carry them here too.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead as _, BufReader};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// The name of the child's step on the command line.
pub const CHILD: &str = "ohne_netz_lauf";
/// The switch that leaves out the reading of the game values (for the counter checks of the other parts).
pub const WITHOUT_VALUES: &str = "--ohne-werte";
/// The file the child leaves its findings in, in the scenario.
const RESULT_FILE: &str = "ergebnis.json";
/// The longest the child may run.
const CHILD_LIMIT: Duration = Duration::from_secs(50 * 60);
/// The longest the watch is given to build the catalog and write the notes.
const WATCH_LIMIT: Duration = Duration::from_secs(35 * 60);
/// So many seconds of the watch's clock pass per tick.
const TICK: f64 = 1.0;
/// The real time between two ticks.
const TICK_REAL: Duration = Duration::from_millis(50);
/// Seconds of the watch's clock it runs on after the last writing of the notes, for a second writing to come.
const SETTLE: f64 = 180.0;
/// Seconds of the watch's clock without a run after which nothing more comes: longer than the five minutes the texts
/// and a failed look at scmdb wait.
const IDLE: f64 = 900.0;
/// The mark of the notes of a contract in the game text.
const NOTES_MARK: &str = "AUFTRAGSANGABEN";
/// The mark of a contract title that gives blueprints.
const BP_MARK: &str = "[BP]";
/// The proxy variables the door to the net reads, in both spellings.
const PROXY_VARS: [&str; 6] = ["ALL_PROXY", "all_proxy", "HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"];
/// The variables that would let a request pass the proxy, or switch the net off the short way, under their names
/// and under the names of the previous version (the program reads those as well).
fn cleared_vars() -> Vec<&'static str> {
    use versekit_core::{envvars, legacy};
    let mut names = vec!["NO_PROXY", "no_proxy", envvars::ENV_NO_NET, envvars::ENV_HOME];
    names.extend([envvars::ENV_NO_NET, envvars::ENV_HOME].into_iter().filter_map(legacy::old_env_name));
    names
}
/// An address of our own the child asks directly, to see that the lock holds.
const LOCK_PROBE_URL: &str = "https://versekit.xharig.com/";
/// The folders of the data copy that are not copied, at any depth: the backups.
const LEFT_OUT: [&str; 1] = ["Sicherungen"];
/// The files of `Intern` that are taken out without `--bestand`: everything read from the game data or fetched from a
/// service of others, so that the run starts as for a player who set VerseKit up but never had them. What the player
/// set up (the chosen translation, the collection, the settings) stays.
const DERIVED: [&str; 28] = [
    "katalog-spieldaten.json",
    "katalog-cache.json",
    "katalog-neubau.lock",
    "auftragsruf.json",
    "rufstufen.json",
    "werte-spieldaten.json",
    "spiel-schiffe.json",
    "spiel-teile.json",
    "signaturen-spieldaten.json",
    "fundorte-spieldaten.json",
    "raffinerie-verfahren-spieldaten.json",
    "crafting-blueprints.json",
    "cig-datenfehler.json",
    "mining-data.json",
    "spielstand.json",
    "laeden-katalog.json",
    "lager-katalog.json",
    "preise.json",
    "erkul-schiffe.json",
    "erkul-teile.json",
    "scmdb-items.json",
    "scmdb-import.json",
    "namen-original.json",
    "titel-original.json",
    "injektion-urtext.json",
    "injektion-urtext@PTU.json", // privacy-ok: a file name of the program, not a mail address
    "injektion-urtext@TECH-PREVIEW.json", // privacy-ok: a file name of the program, not a mail address
    "bauplaene-gesehen.json",
];

// ------------------------------------------------------------ counting the game text

/// What the notes in one `global.ini` amount to.
#[derive(Debug, Default)]
struct Notes {
    /// The keys whose text carries the notes of a contract.
    contracts: BTreeSet<String>,
    /// The keys whose title carries `[BP]`.
    titles: BTreeSet<String>,
    /// How often each kind of line of the notes (`# Ruf`, `# Wartezeit`, …) stands in the file.
    lines: BTreeMap<String, usize>,
}

/// The notes of the text `text` of a `global.ini`.
fn count_notes(text: &str) -> Notes {
    let mut notes = Notes::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        let key = key.split(',').next().unwrap_or(key).trim().to_string();
        if value.contains(BP_MARK) {
            notes.titles.insert(key.clone());
        }
        let Some((_, block)) = value.split_once(NOTES_MARK) else { continue };
        notes.contracts.insert(key);
        for part in block.split("\\n") {
            let part = part.trim_start_matches("<EM4>");
            if let Some(label) = part.strip_prefix("# ").and_then(|rest| rest.split_once(':')).map(|(label, _)| label) {
                *notes.lines.entry(label.trim().to_string()).or_default() += 1;
            }
        }
    }
    notes
}

fn notes_of(file: &str) -> Result<Notes, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    Ok(count_notes(&versekit_core::text::decode_ignore(&bytes)))
}

fn notes_json(notes: &Notes) -> Value {
    json!({
        "auftraege": notes.contracts.iter().collect::<Vec<_>>(),
        "titel": notes.titles.iter().collect::<Vec<_>>(),
        "zeilen": notes.lines,
    })
}

fn notes_from_json(value: &Value) -> Notes {
    let set = |key: &str| -> BTreeSet<String> {
        value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect()
    };
    let lines = value
        .get("zeilen")
        .and_then(Value::as_object)
        .map(|map| map.iter().map(|(k, v)| (k.clone(), v.as_u64().unwrap_or(0) as usize)).collect())
        .unwrap_or_default();
    Notes { contracts: set("auftraege"), titles: set("titel"), lines }
}

/// The keys a note of original texts (`injektion-urtext.json`) holds, with their original text.
fn note_texts(note: &Value) -> serde_json::Map<String, Value> {
    note.get("texte").and_then(Value::as_object).cloned().unwrap_or_default()
}

/// The text file `written` as it was before the notes were written into it, after the note of its original texts
/// `note`: every line the notes touched gets its original text back, and a line they added goes. The lines keep their
/// own ends.
fn pristine(written: &str, note: &Value) -> String {
    let texts = note_texts(note);
    let mut out = String::with_capacity(written.len());
    for line in written.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let end = &line[body.len()..];
        let original = versekit_core::injection::form::split_line(body).and_then(|(key, _, _)| texts.get(key));
        match original.and_then(Value::as_str) {
            Some(text) if text == versekit_core::injection::ADDED => {}
            Some(text) => {
                let head = &body[..body.find('=').unwrap_or(body.len())];
                out.push_str(&format!("{head}={text}{end}"));
            }
            None => out.push_str(line),
        }
    }
    out
}

/// The keys of the lines touched with the net and not without it, counted by the first part of the key
/// (`RepStanding`, `vehicle`, …), and the keys themselves.
fn only_theirs(theirs: &Value, ours: &Value) -> (BTreeMap<String, usize>, Vec<String>) {
    let ours = note_texts(ours);
    let missing: Vec<String> = note_texts(theirs).keys().filter(|key| !ours.contains_key(*key)).cloned().collect();
    let mut kinds = BTreeMap::new();
    for key in &missing {
        let kind = key.split(['_', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9']).next().unwrap_or(key);
        *kinds.entry(kind.trim_start_matches('\u{feff}').to_string()).or_insert(0) += 1;
    }
    (kinds, missing)
}

// ------------------------------------------------------------ the name of the build

/// The name the launcher gives the build of `manifest` (`build_manifest.id`) in its logs `logs`: the last
/// `(SC <CHANNEL> <name>)` whose name ends with `-<channel>.<change>`. Read here on its own, not through the program.
fn launcher_name(manifest: &str, logs: &[String]) -> Option<String> {
    let manifest: Value = serde_json::from_str(manifest).ok()?;
    let change = manifest.pointer("/Data/RequestedP4ChangeNum")?.as_str()?.to_string();
    let pattern = regex::Regex::new(r"\(SC ([A-Z\-]+) ([0-9][^ )]*)\)").ok()?;
    let mut found = None;
    for log in logs {
        for capture in pattern.captures_iter(log) {
            let (channel, name) = (capture[1].to_lowercase(), capture[2].to_string());
            if name.to_lowercase().ends_with(&format!("-{channel}.{change}")) {
                found = Some(name);
            }
        }
    }
    found
}

// ------------------------------------------------------------ the scenario

fn copy_tree(from: &Path, to: &Path, leave_out: &[&str]) -> Result<u64, String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut bytes = 0;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if leave_out.iter().any(|left| name.to_string_lossy().eq_ignore_ascii_case(left)) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            bytes += copy_tree(&entry.path(), &target, leave_out)?;
        } else {
            bytes += std::fs::copy(entry.path(), &target).map_err(|e| format!("{}: {e}", entry.path().display()))?;
        }
    }
    Ok(bytes)
}

/// The first lines of the requests the proxy took.
type Requests = Arc<Mutex<Vec<String>>>;

/// The proxy that takes every request of the child and answers none: the first line of each request is written down.
fn dead_proxy() -> Result<(u16, Requests), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let seen: Requests = Arc::default();
    let written = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut line = String::new();
            let _ = BufReader::new(&stream).read_line(&mut line);
            written.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(line.trim().to_string());
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    Ok((port, seen))
}

/// The host a request line names: `CONNECT host:443 HTTP/1.1` or `GET http://host/… HTTP/1.1`.
fn host_of(line: &str) -> String {
    let target = line.split_whitespace().nth(1).unwrap_or(line);
    let target = target.split("://").nth(1).unwrap_or(target);
    target.split(['/', ':']).next().unwrap_or(target).to_string()
}

fn check(ok: bool, what: &str, failures: &mut Vec<String>) {
    println!("  {} {what}", if ok { "OK    " } else { "FEHLER" });
    if !ok {
        failures.push(what.to_string());
    }
}

fn number(value: &Value, pointer: &str) -> u64 {
    value.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

fn text<'a>(value: &'a Value, pointer: &str) -> &'a str {
    value.pointer(pointer).and_then(Value::as_str).unwrap_or("")
}

/// The switches of a run besides the three folders.
#[derive(Clone, Copy, Default)]
pub struct Options<'a> {
    /// `--vergleich`: a `global.ini` written with the net.
    pub reference: Option<&'a str>,
    /// `--urtext`: the note of the original texts of that file.
    pub note_file: Option<&'a str>,
    /// `--bestand`: the files of `Intern` stay.
    pub keep_caches: bool,
    /// `--uex-stand`: the live build UEX named last.
    pub uex_build: Option<&'a str>,
    /// `--ohne-werte`: the game values are not read.
    pub without_values: bool,
    /// `--kanal`: a side channel beside the main installation (name, its `Data.p4k`, its game folder).
    pub side: Option<(&'a str, &'a str, &'a str)>,
    /// `--gegenprobe`: `sperre` gives the child no proxy.
    pub counter: Option<&'a str>,
}

/// The switch of the child that names the side channel of the scenario.
pub const SIDE_CHANNEL: &str = "--kanal";

/// The folder of the side channel in the scenario with its small files, the text files, and the `Data.p4k` of the
/// real channel as a link (the file is only read, never copied or changed).
fn lay_out_side(scratch: &Path, name: &str, p4k: &str, game: &str) -> Result<PathBuf, String> {
    let folder = scratch.join("Spiel").join(name);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    for file in ["Game.log", "build_manifest.id", "user.cfg"] {
        let from = Path::new(game).join(file);
        if from.is_file() {
            std::fs::copy(&from, folder.join(file)).map_err(|e| format!("{}: {e}", from.display()))?;
        }
    }
    copy_tree(&Path::new(game).join("data"), &folder.join("data"), &[])?;
    let link = folder.join(versekit_core::paths::ARCHIVE_NAME);
    std::fs::hard_link(p4k, &link)
        .or_else(|_| {
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_file(p4k, &link)
            }
            #[cfg(not(windows))]
            {
                std::os::unix::fs::symlink(p4k, &link)
            }
        })
        .map_err(|e| format!("{p4k}: kein Link ins Szenario möglich (dasselbe Laufwerk wie der Temp-Ordner nötig): {e}"))?;
    Ok(folder)
}

pub fn run(archive: &str, game: &str, copy: &str, options: &Options<'_>) -> Result<(), String> {
    let Options { reference, note_file, keep_caches, uex_build, without_values, side, counter } = *options;
    if !Path::new(archive).is_file() {
        return Err(format!("keine Data.p4k: {archive}"));
    }
    let installed =
        Path::new(archive).components().any(|c| c.as_os_str().eq_ignore_ascii_case("Roberts Space Industries"));
    let host = versekit_core::paths::Host::current();
    if installed && versekit_core::game_start::run_state(&versekit_core::paths::Paths::new(host)).blocks_writing() {
        return Err("die Data.p4k gehört zum laufenden Spiel: eine Kopie nehmen oder das Spiel schließen".into());
    }
    let scratch = std::env::temp_dir().join(format!("versekit-ohne-netz-{}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&scratch);
    let data = scratch.join("kopie");
    let channel = scratch.join("Spiel").join("LIVE");
    let machine = scratch.join("maschine");
    std::fs::create_dir_all(&machine).map_err(|e| e.to_string())?;

    // The data copy, and the small files of the game folder.
    let copied = copy_tree(Path::new(copy), &data, &LEFT_OUT)?;
    if !keep_caches {
        let intern = data.join(versekit_core::paths::COPY_DATA_FOLDER).join("Intern");
        for name in DERIVED {
            let _ = std::fs::remove_file(intern.join(name));
        }
        // The data of earlier patches read from the game (`patch-<version>-<channel>.<change>.json`) and the
        // pictures and imports of scmdb go as well.
        for entry in std::fs::read_dir(&intern).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if (name.starts_with("patch-4") || name.starts_with("scmdb-import")) && name.ends_with(".json") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    if let Some(live) = uex_build {
        // The live build UEX named last, as before a patch the player then made without the net.
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
        let file = data
            .join(versekit_core::paths::COPY_DATA_FOLDER)
            .join("Intern")
            .join(versekit_core::stock_fetch::BUILD_FILE);
        let stored = json!({"live": live, "ptu": "", "format": 1, "geholt": now});
        std::fs::write(&file, stored.to_string()).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    std::fs::create_dir_all(&channel).map_err(|e| e.to_string())?;
    for file in ["Game.log", "build_manifest.id", "user.cfg"] {
        let from = Path::new(game).join(file);
        if from.is_file() {
            std::fs::copy(&from, channel.join(file)).map_err(|e| format!("{}: {e}", from.display()))?;
        }
    }
    copy_tree(&Path::new(game).join("data"), &channel.join("data"), &[])?;
    if let Some((name, p4k, folder)) = side {
        lay_out_side(&scratch, name, p4k, folder)?;
    }
    // Notes of original texts the copy holds belong to the files of the real installation, not to the copies here.
    for entry in std::fs::read_dir(data.join(versekit_core::paths::COPY_DATA_FOLDER).join("Intern"))
        .into_iter()
        .flatten()
        .flatten()
    {
        if entry.file_name().to_string_lossy().starts_with("injektion-urtext") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    // With the note of the compared file its translation stands in the copy as it was before any notes.
    let note: Option<Value> = match note_file {
        Some(file) => Some(
            serde_json::from_slice(&std::fs::read(file).map_err(|e| format!("{file}: {e}"))?)
                .map_err(|e| format!("{file}: {e}"))?,
        ),
        None => None,
    };
    if let (Some(note), Some(reference)) = (&note, reference) {
        let written =
            versekit_core::text::decode_ignore(&std::fs::read(reference).map_err(|e| format!("{reference}: {e}"))?);
        let language =
            Path::new(reference).parent().and_then(Path::file_name).ok_or("kein Sprachordner im Vergleich")?;
        let target = channel.join("data").join("Localization").join(language).join("global.ini");
        std::fs::write(&target, pristine(&written, note)).map_err(|e| format!("{}: {e}", target.display()))?;
        println!("global.ini aus dem Vergleich ohne seine {} Angaben-Zeilen hergestellt", note_texts(note).len());
    }
    // The launcher's logs, where the copy keeps its configuration folder and where the machine of the child keeps
    // it: the reading of the build name asks the paths of the process, not those of the copy.
    let mut logs = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        let from = Path::new(&appdata).join("rsilauncher").join("logs");
        let places = [data.join("AppData"), machine.join("AppData").join("Roaming")];
        for to in places.iter().map(|base| base.join("rsilauncher").join("logs")) {
            std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
            for name in ["log.old.log", "log.log"] {
                let _ = std::fs::copy(from.join(name), to.join(name));
            }
        }
        for name in ["log.old.log", "log.log"] {
            if let Ok(bytes) = std::fs::read(from.join(name)) {
                logs.push(String::from_utf8_lossy(&bytes).into_owned());
            }
        }
    }
    let manifest = std::fs::read_to_string(Path::new(archive).with_file_name("build_manifest.id"))
        .or_else(|_| std::fs::read_to_string(channel.join("build_manifest.id")))
        .unwrap_or_default();
    let expected = launcher_name(&manifest, &logs);
    println!(
        "Szenario {} ({} MB Datenkopie{}), Spielstand laut Launcher {}",
        scratch.display(),
        copied / 1_048_576,
        if keep_caches { ", mit Zwischenspeichern" } else { ", ohne Zwischenspeicher" },
        expected.as_deref().unwrap_or("unbekannt")
    );

    let (port, seen) = dead_proxy()?;
    let proxy = format!("http://127.0.0.1:{port}");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = Command::new(exe);
    child.arg(CHILD).arg(&scratch).arg(archive);
    if without_values {
        child.arg(WITHOUT_VALUES);
    }
    if let Some((name, _, _)) = side {
        child.arg(SIDE_CHANNEL).arg(name);
    }
    for name in cleared_vars().iter().chain(PROXY_VARS.iter()) {
        child.env_remove(name);
    }
    if counter != Some("sperre") {
        for name in PROXY_VARS {
            child.env(name, &proxy);
        }
    }
    child
        .env(versekit_core::paths::ENV_TEST_MACHINE, &machine)
        .env(versekit_core::paths::ENV_INSTALL_DIR, &channel)
        .env(versekit_core::game_start::ENV_NO_PROCESS, "1");
    let started = Instant::now();
    let mut running = child.spawn().map_err(|e| e.to_string())?;
    let status = loop {
        if let Some(status) = running.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if started.elapsed() > CHILD_LIMIT {
            let _ = running.kill();
            return Err(format!("der Lauf brauchte länger als {} min", CHILD_LIMIT.as_secs() / 60));
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    if !status.success() {
        return Err(format!("der Lauf brach ab: {status}"));
    }
    let result: Value = serde_json::from_slice(&std::fs::read(scratch.join(RESULT_FILE)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let requests = seen.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let judged = judge(&result, &requests, expected.as_deref(), reference, note.as_ref());
    if std::env::var_os("VERSEKIT_OHNE_NETZ_BEHALTEN").is_none() {
        let _ = std::fs::remove_dir_all(&scratch);
    } else {
        println!("behalten: {}", scratch.display());
    }
    judged
}

fn judge(
    result: &Value,
    requests: &[String],
    expected: Option<&str>,
    reference: Option<&str>,
    note: Option<&Value>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    println!(
        "Lauf: {} s Wache ({} s auf ihrer Uhr), {} s Werte",
        number(result, "/wache/sekunden"),
        number(result, "/wache/uhr"),
        number(result, "/werte/sekunden")
    );

    println!("Netzsperre");
    let lock = text(result, "/sperre/antwort");
    check(
        text(result, "/sperre/ergebnis") == "fehler",
        &format!("direkte Anfrage an {LOCK_PROBE_URL} scheitert ({lock})"),
        &mut failures,
    );
    check(
        !lock.contains(versekit_core::envvars::ENV_NO_NET),
        "gescheitert an der Verbindung, nicht am Schalter VERSEKIT_NO_NET",
        &mut failures,
    );
    check(
        !requests.is_empty(),
        &format!("{} Anfragen liefen in die tote Proxy-Adresse", requests.len()),
        &mut failures,
    );
    let mut hosts: BTreeMap<String, usize> = BTreeMap::new();
    for request in requests {
        *hosts.entry(host_of(request)).or_default() += 1;
    }
    for (host, count) in &hosts {
        println!("         angefragt: {host} ({count}×)");
    }

    println!("1. Bauplan-Katalog");
    let catalog = &result["katalog"];
    let version = text(catalog, "/version");
    check(catalog["da"] == true, &format!("{} geschrieben", versekit_gamedata::list::CACHE_FILE), &mut failures);
    check(
        number(catalog, "/format") == versekit_gamedata::list::CACHE_FORMAT as u64,
        &format!("Format {} (vorher {})", number(catalog, "/format"), catalog["format_vorher"]),
        &mut failures,
    );
    check(number(catalog, "/bauplaene") >= 1000, &format!("{} Baupläne", number(catalog, "/bauplaene")), &mut failures);
    let missions = number(catalog, "/auftraege");
    let keyed = number(catalog, "/auftraege_mit_schluessel");
    check(
        missions > 0 && keyed == missions,
        &format!("{keyed} von {missions} Aufträgen mit Textschlüssel"),
        &mut failures,
    );
    match expected {
        Some(name) => check(version == name, &format!("Stand {version} wie im Launcher-Log ({name})"), &mut failures),
        None => println!("  ?      Stand {version} (kein Launcher-Log zum Vergleich)"),
    }
    let scmdb = text(catalog, "/scmdb");
    println!("  Info   scmdb-Teil: {}", if scmdb.is_empty() { "fehlt (ohne Netz erwartet)" } else { scmdb });

    println!("2. Auftragsangaben im Spieltext");
    let notes = notes_from_json(&result["spieltext"]["nachher"]);
    check(
        number(result, "/spieltext/vorher") == 0 && result["spieltext"]["vorher_ergaenzt"] == false,
        "die Kopie der global.ini hatte vorher keine Angaben und keine Ergänzung von VerseKit",
        &mut failures,
    );
    check(
        !notes.contracts.is_empty(),
        &format!(
            "{} Aufträge mit {NOTES_MARK} ({} Einspielungen gemeldet)",
            notes.contracts.len(),
            number(result, "/wache/eingespielt")
        ),
        &mut failures,
    );
    check(!notes.titles.is_empty(), &format!("{} Titel mit {BP_MARK}", notes.titles.len()), &mut failures);
    check(
        number(result, "/spieltext/zeilen") > 0,
        &format!("{} Zeilen geändert (Urtext-Notiz)", number(result, "/spieltext/zeilen")),
        &mut failures,
    );
    let theirs = match reference {
        Some(file) => Some(notes_of(file)?),
        None => None,
    };
    if let Some(theirs) = &theirs {
        // A title with several descriptions carries the notes in one of them, and the game data and scmdb may choose
        // another one: the descriptions count by number, the titles one by one.
        let missing: Vec<&String> = theirs.contracts.difference(&notes.contracts).collect();
        let missing_titles: Vec<&String> = theirs.titles.difference(&notes.titles).collect();
        check(
            notes.contracts.len() >= theirs.contracts.len(),
            &format!("{} Auftragstexte mit Angaben, Vergleich {}", notes.contracts.len(), theirs.contracts.len()),
            &mut failures,
        );
        println!(
            "  Info   Auftragstexte nur im Vergleich mit Angaben: {} {:?}",
            missing.len(),
            missing.iter().take(8).collect::<Vec<_>>()
        );
        check(
            missing_titles.is_empty(),
            &format!(
                "alle {} {BP_MARK}-Titel aus dem Vergleich (fehlen: {} {:?})",
                theirs.titles.len(),
                missing_titles.len(),
                missing_titles
            ),
            &mut failures,
        );
        let extra = notes.contracts.difference(&theirs.contracts).count();
        println!("  Info   ohne Netz zusätzlich: {extra} Aufträge mit Angaben");
    }
    if let Some(note) = note {
        let (kinds, missing) = only_theirs(note, &result["spieltext"]["urtext"]);
        println!(
            "  Info   Zeilen mit Angaben: ohne Netz {}, mit Netz {}; nur mit Netz {} nach Art {kinds:?}",
            number(result, "/spieltext/zeilen"),
            note_texts(note).len(),
            missing.len()
        );
        println!("  Info   davon zum Beispiel {:?}", missing.iter().take(12).collect::<Vec<_>>());
    }

    println!("3. Ruf-Angaben");
    let reputation = &result["ruf"];
    let table = text(reputation, "/version");
    check(
        versekit_core::injection::reputation::same_build(table, version),
        &format!("Ruf-Tabelle {table} zum Katalog {version}"),
        &mut failures,
    );
    check(
        number(reputation, "/auftraege") > 0,
        &format!("{} Aufträge mit Ruf in der Tabelle", number(reputation, "/auftraege")),
        &mut failures,
    );
    check(
        number(reputation, "/bedingungen") > 0,
        &format!("{} Aufträge mit Bedingungen ({})", number(reputation, "/bedingungen"), text(reputation, "/quelle")),
        &mut failures,
    );
    check(number(reputation, "/stufen") > 0, &format!("{} Ruf-Stufen", number(reputation, "/stufen")), &mut failures);
    check(
        number(reputation, "/regionen") > 0,
        &format!("{} Regionen in der Tabelle (aus der Sternkarte)", number(reputation, "/regionen")),
        &mut failures,
    );
    let game_version = text(result, "/spielstand");
    let uex = text(result, "/spielstand_uex");
    check(
        game_version.is_empty() || versekit_core::injection::reputation::same_build(table, game_version),
        &format!(
            "Ruf gilt für den Spielstand der Angaben ({}; UEX: {})",
            if game_version.is_empty() { "unbekannt" } else { game_version },
            if uex.is_empty() { "unbekannt" } else { uex }
        ),
        &mut failures,
    );
    for label in ["Ruf", "Max. Reputation", "Region"] {
        let ours = notes.lines.get(label).copied().unwrap_or(0);
        let wanted = theirs.as_ref().map(|t| t.lines.get(label).copied().unwrap_or(0));
        let ok = ours > 0 && wanted.is_none_or(|wanted| ours >= wanted);
        let compared = wanted.map(|w| format!(" (Vergleich {w})")).unwrap_or_default();
        check(ok, &format!("{ours} Zeilen „# {label}“{compared}"), &mut failures);
    }

    println!("4. Werte");
    let values = &result["werte"];
    if values["ausgelassen"] == true {
        check(false, &format!("nicht gelesen ({WITHOUT_VALUES})"), &mut failures);
    } else {
        check(text(values, "/quelle") == "archive", &format!("Werte aus {}", text(values, "/quelle")), &mut failures);
        check(
            number(values, "/gegenstaende") >= 1000,
            &format!("{} Gegenstände", number(values, "/gegenstaende")),
            &mut failures,
        );
        check(number(values, "/rezepte") > 0, &format!("{} Rezepte", number(values, "/rezepte")), &mut failures);
        check(
            number(values, "/schiffe") >= 100,
            &format!("{} Schiffe mit Steckplätzen", number(values, "/schiffe")),
            &mut failures,
        );
        check(
            number(values, "/schiffe_mit_werten") >= 100,
            &format!("{} Schiffe mit Grundwerten", number(values, "/schiffe_mit_werten")),
            &mut failures,
        );
        check(number(values, "/teile") > 0, &format!("{} Ausstattungsteile", number(values, "/teile")), &mut failures);
        check(values["raffinerie"] == true, "Raffinerie-Methoden aus den Spieldaten", &mut failures);
        check(values["signaturen"] == true, "Gesteins-Signaturen aus den Spieldaten", &mut failures);
        check(
            number(values, "/abbau_felsen") > 0,
            &format!("{} Felsen in den Abbau-Daten", number(values, "/abbau_felsen")),
            &mut failures,
        );
    }

    let channel = &result["kanal"];
    if !channel.is_null() {
        println!("5. Seitenkanal {} mit den eigenen Daten", text(channel, "/name"));
        let theirs = notes_from_json(&channel["angaben"]);
        let own_version = text(channel, "/katalog_version");
        check(channel["katalog_da"] == true, "Katalog des Kanals im eigenen Datenordner", &mut failures);
        check(
            number(channel, "/auftraege") > 0,
            &format!("{} Aufträge im Katalog des Kanals (Stand {own_version})", number(channel, "/auftraege")),
            &mut failures,
        );
        check(
            versekit_core::injection::reputation::same_build(text(channel, "/ruf_version"), own_version),
            &format!("Ruf-Tabelle des Kanals {} zu seinem Katalog {own_version}", text(channel, "/ruf_version")),
            &mut failures,
        );
        check(channel["werte_da"] == true, "Werte des Kanals im eigenen Datenordner", &mut failures);
        check(
            !theirs.contracts.is_empty(),
            &format!(
                "{} Aufträge mit {NOTES_MARK} in der Textdatei des Kanals, {} Titel mit {BP_MARK} (Hauptkanal: {} und {})",
                theirs.contracts.len(),
                theirs.titles.len(),
                notes.contracts.len(),
                notes.titles.len()
            ),
            &mut failures,
        );
        println!("  Info   Stand des Hauptkanals {version}, des Kanals {own_version}; Einsetzen: {}", text(channel, "/einsetzen"));
    }

    println!("Was ohne Netz fehlt (Dateien, die nur ein Fremddienst füllt)");
    if let Some(files) = result["fremd"].as_object() {
        for (file, there) in files {
            println!("  {} {file}", if there == true { "da    " } else { "fehlt " });
        }
    }
    if let Some(theirs) = &theirs {
        println!("Zeilenarten der Angaben: ohne Netz / Vergleich");
        let labels: BTreeSet<&String> = notes.lines.keys().chain(theirs.lines.keys()).collect();
        for label in labels {
            let (ours, wanted) =
                (notes.lines.get(label).copied().unwrap_or(0), theirs.lines.get(label).copied().unwrap_or(0));
            println!("  {} # {label}: {ours} / {wanted}", if ours >= wanted { "      " } else { "WENIGER" });
        }
    }
    println!("Gemeldete Fehler des Laufs (Stelle: wie oft)");
    let mut reported: BTreeMap<String, (usize, String)> = BTreeMap::new();
    for entry in result["fehler"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let (label, said) = entry.split_once(": ").unwrap_or((entry, ""));
        let seen = reported.entry(label.to_string()).or_insert((0, said.replace('\n', " ")));
        seen.0 += 1;
    }
    for (label, (count, said)) in &reported {
        println!("  {label}: {count}× (zuletzt so: {said})");
    }
    let denied = result["abgelehnt"].as_array().map_or(0, Vec::len);
    check(denied == 0, &format!("keine Schreibversuche außerhalb des Szenarios ({denied})"), &mut failures);

    if failures.is_empty() {
        println!("ERGEBNIS vollständig ohne Netz");
        Ok(())
    } else {
        Err(format!("ERGEBNIS {} Prüfungen fehlgeschlagen: {failures:?}", failures.len()))
    }
}

// ------------------------------------------------------------ the child

/// What the child collects of the errors the program reports.
static REPORTED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn json_file(path: &str) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn entries(value: Option<&Value>) -> usize {
    match value {
        Some(Value::Object(map)) => map.len(),
        Some(Value::Array(list)) => list.len(),
        _ => 0,
    }
}

pub fn child(scratch: &str, archive: &str, without_values: bool, side: Option<&str>) -> Result<(), String> {
    use versekit_core::game_texts::Notice;
    use versekit_core::net::{Net as _, Web};
    use versekit_core::paths::{COPY_DATA_FOLDER, Host, Paths};
    use versekit_core::{errors, game_ini, injection, translation, writeguard};
    use versekit_gamedata::values_store;

    let root = Path::new(scratch).join("kopie");
    let paths = Paths::for_copy(Host::current(), &root.to_string_lossy()).ok_or("keine Datenkopie im Szenario")?;
    paths.set_given_archive(archive, versekit_core::given::GivenBy::CommandLine);
    writeguard::deny(true);
    writeguard::allow_under(Some(scratch));
    errors::set_sink(|label, text| {
        REPORTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(format!("{label}: {text}"))
    });
    let catalog_file = paths.app_file(versekit_gamedata::list::CACHE_FILE);
    let format_before =
        json_file(&catalog_file).and_then(|c| c.pointer("/quelle/format").cloned()).unwrap_or(Value::Null);

    let choice = game_ini::ini_choice(&paths);
    let ini = choice.path.clone().ok_or("keine global.ini im Spielordner des Szenarios")?;
    let before = notes_of(&ini)?.contracts.len();
    let applied_before = injection::is_applied(&ini);

    // The lock: one request straight through the door to the net.
    let door = Web::new(translation::USER_AGENT, Duration::from_secs(20), 1 << 20);
    let lock = match door.get(LOCK_PROBE_URL) {
        Ok(body) => json!({"ergebnis": "erreicht", "antwort": format!("{} Bytes", body.len())}),
        Err(error) => json!({"ergebnis": "fehler", "antwort": error.to_string()}),
    };

    // The background as the overlay runs it.
    let net = Arc::new(Web::new(
        translation::USER_AGENT,
        Duration::from_secs(translation::TIMEOUT_SECONDS),
        200 * 1024 * 1024,
    ));
    let mut watch = versekit_gamedata::texts_watch::watch(net);
    let shared = Arc::new(paths);
    let started = Instant::now();
    let (mut clock, mut applied, mut last_applied, mut told) = (0.0_f64, 0usize, None::<f64>, Vec::new());
    let mut idle_since = 0.0_f64;
    loop {
        for notice in watch.tick(&shared, clock) {
            if let Notice::Applied(_) = notice {
                applied += 1;
                last_applied = Some(clock);
            }
            told.push(format!("{notice:?}"));
        }
        if watch.busy() {
            idle_since = clock;
        }
        // Written and quiet since, or nothing going on for longer than every clock of the watch waits.
        let settled =
            !watch.busy() && (last_applied.is_some_and(|at| clock - at >= SETTLE) || clock - idle_since >= IDLE);
        if settled || started.elapsed() > WATCH_LIMIT {
            break;
        }
        clock += TICK;
        std::thread::sleep(TICK_REAL);
    }
    let watch_seconds = started.elapsed().as_secs();
    while watch.busy() {
        let _ = watch.tick(&shared, clock);
        std::thread::sleep(TICK_REAL);
    }
    let paths = shared;
    // A copy of the data only looks at the side channels in the watch; their notes are written as the button
    // "Neu einsetzen" does, which takes the same way through the data of the channel's own archive.
    let side_outcome = side.map(|_| {
        let version = versekit_gamedata::texts_watch::game_version(&paths);
        let said = versekit_core::game_texts::apply(&paths, &versekit_gamedata::texts_watch::GameArchive, &version);
        told.push(format!("Neu einsetzen: {said:?}"));
        format!("{said:?}")
    });

    // The values as the pages start them.
    let values_started = Instant::now();
    let folder = Path::new(archive).parent().map(Path::to_path_buf);
    let supply = Arc::new(values_store::Runtime::for_host(&paths.host));
    let start = values_store::start(folder.as_deref(), &paths.app_file(values_store::CACHE_FILE), supply);
    let built = start.rebuild.as_ref().filter(|_| !without_values).and_then(values_store::Rebuild::run);
    let (source, items, recipes) = match (&built, &start.values) {
        (Some(built), _) => {
            (format!("{:?}", built.source).to_lowercase(), built.values.items.len(), built.values.recipes.len())
        }
        (None, Some(values)) => (start.source.clone(), values.items.len(), values.recipes.len()),
        (None, None) => (String::new(), 0, 0),
    };
    let ships = json_file(&paths.app_file(versekit_gamedata::ship_slots::SHIPS_FILE));
    let ship_list = ships.as_ref().and_then(|s| s.get("schiffe")).or(ships.as_ref());
    let with_values = match ship_list {
        Some(Value::Object(map)) => map.values().filter(|ship| ship.get("werte").is_some()).count(),
        Some(Value::Array(list)) => list.iter().filter(|ship| ship.get("werte").is_some()).count(),
        _ => 0,
    };
    let parts = json_file(&paths.app_file(versekit_gamedata::ship_slots::PARTS_FILE));
    let rocks = json_file(&paths.app_file(versekit_core::mining_data::ROCK_SIGNATURES_FILE));
    let values_seconds = values_started.elapsed().as_secs();

    let catalog = json_file(&catalog_file);
    let missions = catalog.as_ref().and_then(|c| c.get("missionen")).and_then(Value::as_object);
    let keyed = missions.map_or(0, |m| {
        m.values().filter(|m| m.get("titel_key").and_then(Value::as_str).is_some_and(|k| !k.is_empty())).count()
    });
    let table = json_file(&paths.app_file(injection::reputation::CACHE_FILE));
    let ranks = json_file(&paths.app_file(injection::ranks::CACHE_FILE));
    let origtext = json_file(&injection::origtext_path(&paths, &ini));
    let after = notes_of(&ini)?;
    let foreign = [
        versekit_core::recipe_file::FILE,
        versekit_core::recipe_file::ISSUES_FILE,
        versekit_core::mining_data::FILE,
        versekit_core::stock_fetch::BUILD_FILE,
        versekit_core::injection::sources::SHOPS_FILE,
    ]
    .iter()
    .map(|name| ((*name).to_string(), json!(Path::new(&paths.app_file(name)).is_file())))
    .collect::<serde_json::Map<_, _>>();
    let game_version = versekit_gamedata::texts_watch::game_version(&paths);
    let uex_version = versekit_core::stock_fetch::live_build(&paths);
    let rock_count = entries(rocks.as_ref().and_then(|r| r.get("steine")));
    let side_result = side.map(|name| {
        let mut found = side_findings(&paths, scratch, name);
        found["einsetzen"] = json!(side_outcome.clone().unwrap_or_default());
        found
    });

    let result = json!({
        "sperre": lock,
        "wache": {"sekunden": watch_seconds, "uhr": clock as u64, "eingespielt": applied, "meldungen": told},
        "katalog": {
            "da": catalog.is_some(),
            "format": catalog.as_ref().and_then(|c| c.pointer("/quelle/format")).and_then(Value::as_u64).unwrap_or(0),
            "format_vorher": format_before,
            "version": catalog.as_ref().and_then(|c| c.get("version")).and_then(Value::as_str).unwrap_or(""),
            "scmdb": catalog.as_ref().and_then(|c| c.pointer("/quelle/scmdb")).and_then(Value::as_str).unwrap_or(""),
            "bauplaene": entries(catalog.as_ref().and_then(|c| c.get("bauplaene"))),
            "auftraege": missions.map_or(0, serde_json::Map::len),
            "auftraege_mit_schluessel": keyed,
        },
        "spieltext": {
            "datei": ini,
            "sprache": choice.language,
            "vorher": before,
            "vorher_ergaenzt": applied_before,
            "nachher": notes_json(&after),
            "zeilen": entries(origtext.as_ref().and_then(|o| o.get("texte"))),
            "urtext": origtext.clone().unwrap_or(Value::Null),
        },
        "ruf": {
            "version": table.as_ref().and_then(|t| t.get("version")).and_then(Value::as_str).unwrap_or(""),
            "auftraege": entries(table.as_ref().and_then(|t| t.get("auftraege"))),
            "bedingungen": entries(table.as_ref().and_then(|t| t.get("angaben"))),
            "quelle": table.as_ref().and_then(|t| t.get("angaben_quelle")).and_then(Value::as_str).unwrap_or(""),
            "regionen": entries(table.as_ref().and_then(|t| t.get("regionen"))),
            "stufen": entries(ranks.as_ref().and_then(|r| r.get("stufen")).or(ranks.as_ref())),
        },
        "spielstand": game_version,
        "spielstand_uex": uex_version,
        "werte": {
            "ausgelassen": without_values,
            "sekunden": values_seconds,
            "quelle": source,
            "gegenstaende": items,
            "rezepte": recipes,
            "schiffe": entries(ship_list),
            "schiffe_mit_werten": with_values,
            "teile": entries(parts.as_ref().and_then(|p| p.get("teile")).or(parts.as_ref())),
            "raffinerie": Path::new(&paths.app_file(versekit_gamedata::refining_processes::FILE)).is_file(),
            "signaturen": rocks.is_some(),
            "abbau_felsen": rock_count,
        },
        "fremd": foreign,
        "kanal": side_result.unwrap_or(Value::Null),
        "fehler": REPORTED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone(),
        "abgelehnt": writeguard::take_denied(),
        "datenordner": root.join(COPY_DATA_FOLDER).to_string_lossy(),
    });
    writeguard::deny(false);
    let out = PathBuf::from(scratch).join(RESULT_FILE);
    std::fs::write(&out, serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// What the run left of the side channel `name`: its own catalog (in its own data folder), the notes in its text
/// files, and whether the files of the main installation stayed apart from its data.
fn side_findings(paths: &versekit_core::paths::Paths, scratch: &str, name: &str) -> Value {
    let root = Path::new(scratch).join("Spiel").join(name);
    let own = paths.for_channel(name, &root.to_string_lossy());
    let catalog = json_file(&own.app_file(versekit_gamedata::list::CACHE_FILE));
    let missions = catalog.as_ref().and_then(|c| c.get("missionen")).and_then(Value::as_object);
    let table = json_file(&own.app_file(versekit_core::injection::reputation::CACHE_FILE));
    // The text file with the most notes: the one the channel's language is kept in.
    let mut best: Option<(String, Notes)> = None;
    let languages = root.join("data").join("Localization");
    for entry in std::fs::read_dir(&languages).into_iter().flatten().flatten() {
        let file = entry.path().join("global.ini");
        if let Ok(notes) = notes_of(&file.to_string_lossy())
            && best.as_ref().is_none_or(|(_, seen)| notes.contracts.len() > seen.contracts.len())
        {
            best = Some((file.to_string_lossy().into_owned(), notes));
        }
    }
    let (file, notes) = best.unwrap_or_default();
    json!({
        "name": name,
        "katalog_da": catalog.is_some(),
        "katalog_version": catalog.as_ref().and_then(|c| c.get("version")).and_then(Value::as_str).unwrap_or(""),
        "auftraege": missions.map_or(0, serde_json::Map::len),
        "ruf_version": table.as_ref().and_then(|t| t.get("version")).and_then(Value::as_str).unwrap_or(""),
        "ruf_auftraege": entries(table.as_ref().and_then(|t| t.get("auftraege"))),
        "datei": file,
        "angaben": notes_json(&notes),
        "werte_da": Path::new(&own.app_file(versekit_gamedata::values_store::CACHE_FILE)).is_file(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notes_of_a_game_text_are_counted_by_contract_title_and_kind_of_line() {
        let text = "A_Desc=Text\\n\\n<EM4>AUFTRAGSANGABEN</EM4>\\n\\n<EM4># Max. Reputation: X</EM4>\\n# Wartezeit: 5\\n<EM4># Ruf: Y</EM4>\n\
                    A_Title=Titel <EM4>[BP]</EM4>\nB_Desc,P=Nichts\nC_Desc=\\nAUFTRAGSANGABEN\\n# Ruf: Z\n";
        let notes = count_notes(text);
        assert_eq!(notes.contracts.iter().collect::<Vec<_>>(), ["A_Desc", "C_Desc"]);
        assert_eq!(notes.titles.iter().collect::<Vec<_>>(), ["A_Title"]);
        assert_eq!(notes.lines.get("Ruf"), Some(&2));
        assert_eq!(notes.lines.get("Max. Reputation"), Some(&1));
        assert_eq!(notes.lines.get("Wartezeit"), Some(&1));
    }

    #[test]
    fn the_build_is_named_from_the_launcher_log_by_the_change_number() {
        let manifest = r#"{"Data": {"Branch": "sc-alpha-4.10.0", "RequestedP4ChangeNum": "13165946"}}"#;
        let old = "Starting delta update (SC LIVE 4.10.2-live.12913470) in C:\\x".to_string();
        let new =
            "Starting delta update (SC LIVE 4.10.3-live.13165946) in C:\\x\nSC PTU 4.11.0-ptu.13200000".to_string();
        assert_eq!(launcher_name(manifest, &[old.clone(), new]).as_deref(), Some("4.10.3-live.13165946"));
        assert_eq!(launcher_name(manifest, &[old]), None);
    }

    #[test]
    fn the_file_before_the_notes_comes_back_from_the_note_of_its_original_texts() {
        let note = json!({"texte": {
            "\u{feff}Menu": "Alpha",
            "A_Title": "Titel",
            "Rank1": "Rang",
            "item_Name_X": versekit_core::injection::ADDED,
        }});
        let written = "\u{feff}Menu=Alpha\\nVerseKit\r\nA_Title=Titel <EM4>[BP]</EM4>\r\nRank1,P=Rang [ab 1]\r\nitem_Name_X=X\r\nOther=Rest";
        assert_eq!(pristine(written, &note), "\u{feff}Menu=Alpha\r\nA_Title=Titel\r\nRank1,P=Rang\r\nOther=Rest");
        let ours = json!({"texte": {"A_Title": "Titel"}});
        let (kinds, missing) = only_theirs(&note, &ours);
        assert_eq!(missing.len(), 3);
        assert_eq!(kinds.get("Rank"), Some(&1));
        assert_eq!(kinds.get("Menu"), Some(&1), "the mark at the start of the file does not count: {kinds:?}");
    }

    #[test]
    fn the_host_of_a_request_line_is_found_for_both_kinds_of_request() {
        assert_eq!(host_of("CONNECT scmdb.net:443 HTTP/1.1"), "scmdb.net");
        assert_eq!(host_of("GET http://example.org/a/b HTTP/1.1"), "example.org");
    }
}
