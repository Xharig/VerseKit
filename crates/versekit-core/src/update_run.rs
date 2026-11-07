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

//! One update run (4.12 `update_run`): the lock against two runs at once, the run mark the next start reads,
//! the result the helper writes, and handing over to the helper under Windows.
//!
//! The file names and fields are those of 4.12: the next start of this version reads what the helper of 4.12
//! left behind after the jump to it.
//!
//! * **Run mark** (`update-lauf.json`), written before the program steps down. The next start says what became
//!   of the update; ⚠ the version of the started program decides, not the exit code of the installer.
//! * **Lock** (`update-sperre.json`), created only if it is not there. ⚠ Failing to create it does not stop the
//!   update: the lock guards against a double run, the checksum against a foreign file.
//! * **Helper**: a copy of this program file in the temporary folder, started with [`HELPER_SWITCH`]. It waits
//!   until every process of the program file has ended, compares the checksum once more, runs the installer
//!   and starts the program again (`update_helper`).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::language::{self, Arg};
use crate::paths::Paths;
use crate::updater::{self, is_ours};
use crate::{errors, jsonio, legacy};

pub const RUN_FILE: &str = "update-lauf.json";
pub const RESULT_FILE: &str = "update-ergebnis.txt";
pub const LOCK_FILE: &str = "update-sperre.json";
pub const LOG_FILE: &str = "update-helfer.txt";
pub const LOG_FILE_OLD: &str = "update-helfer.1.txt";
pub const SETUP_LOG: &str = "update-setup.txt";
/// The copy of the program file that runs as the helper, in the temporary folder.
pub const HELPER_NAME: &str = "versekit-update-helfer.exe";
/// The switch that makes the program the helper.
pub const HELPER_SWITCH: &str = "--update-helfer";
/// The beginning of the variables the helper gets its job through. ⚠ Through the environment, not a file:
/// a file in the temporary folder could be replaced together with the installer, the checksum with it.
pub const ENV_PREFIX: &str = "VERSEKIT_UPDATE_";

/// After these exit codes of the installer the helper starts the program again: 0 installed, 2 cancelled before
/// anything changed, 3 failed before installing, 5 cancelled while installing (Inno rolls back). Any other
/// code starts nothing; the next start by hand tells it from the run mark.
pub const RESTART_AFTER: [i32; 4] = [0, 2, 3, 5];
/// The helper's own exit codes, far from those of the installer.
pub const RC_CHECKSUM_BAD: i32 = 90;
pub const RC_OLD_STUCK: i32 = 91;
/// How long the helper waits for the old program to end, in seconds.
pub const WAIT_SECONDS: u64 = 60;
/// A lock older than this is orphaned, even if its process number lives (Windows hands numbers out again).
pub const LOCK_MAX_AGE: f64 = 15.0 * 60.0;
/// A run mark older than this goes without a word.
pub const RUN_MAX_AGE: f64 = 24.0 * 3600.0;
/// The setting that remembers an automatic update that failed (`version|time`).
pub const FAILED_SETTING: &str = "auto_update_fehlschlag";
/// So long a version is not installed again by itself after its automatic update failed.
pub const FAILED_PAUSE: f64 = 24.0 * 3600.0;

fn json_read(path: &str) -> Option<serde_json::Map<String, Value>> {
    let bytes = fs::read(path).ok()?;
    match serde_json::from_slice::<Value>(&bytes).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn json_write(path: &str, value: &Value) -> io::Result<()> {
    jsonio::write_text_atomic(path, &value.to_string(), jsonio::Newline::Keep)
}

fn remove(path: &str) {
    match jsonio::remove_file(path) {
        Err(error) if !jsonio::is_not_found(&error) => errors::record("update_run.remove", &error),
        _ => {}
    }
}

/// `v5.0.0 ` → `5.0.0`, for comparing versions as the run mark holds them.
pub fn norm(version: &str) -> String {
    crate::text::py_strip(version).to_lowercase().trim_start_matches('v').to_string()
}

// ------------------------------------------------------------ processes

/// Does this process live? An error when asking counts as no.
pub fn pid_alive(pid: i64) -> bool {
    let Ok(pid) = u32::try_from(pid) else { return false };
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, GetLastError, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: a handle that is closed below; no other memory is handed over.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            // Access denied means it is there, only closed to us.
            // SAFETY: reads the error of this thread.
            return unsafe { GetLastError() } == ERROR_ACCESS_DENIED;
        }
        let mut code = 0u32;
        // SAFETY: the handle from above and a place for the code.
        let asked = unsafe { GetExitCodeProcess(handle, &raw mut code) } != 0;
        // SAFETY: the handle from above, closed once.
        unsafe { CloseHandle(handle) };
        !asked || code == STILL_ACTIVE as u32
    }
    #[cfg(unix)]
    {
        let Ok(pid) = libc::pid_t::try_from(pid) else { return false };
        // SAFETY: signal 0 only asks whether the process is there.
        if unsafe { libc::kill(pid, 0) } == 0 {
            return true;
        }
        io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}

// ------------------------------------------------------------ lock

fn orphaned(path: &str, now: f64) -> bool {
    let Some(data) = json_read(path) else {
        // Unreadable: either somebody writes it right now (then it is a moment old) or it is left over.
        return fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .is_none_or(|m| now - m.as_secs_f64() > 10.0);
    };
    let Some(time) = data.get("zeit").and_then(Value::as_f64) else { return true };
    if now - time > LOCK_MAX_AGE {
        return true;
    }
    !data.get("pid").and_then(Value::as_i64).is_some_and(pid_alive)
}

/// May this process update (true), or is another update under way (false)? ⚠ When the lock cannot even be
/// created (disk full, no rights) the update is not stopped.
pub fn take_lock(paths: &Paths, pid: u32, now: f64) -> bool {
    let path = paths.app_file(LOCK_FILE);
    for _ in 0..2 {
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                use std::io::Write;
                let _ = file.write_all(json!({"pid": pid, "zeit": now}).to_string().as_bytes());
                return true;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !orphaned(&path, now) {
                    return false;
                }
                remove(&path);
            }
            Err(error) => {
                errors::record("update_run.take_lock", &error);
                return true;
            }
        }
    }
    false
}

/// The lock belongs to the helper from now on: the program steps down in a moment.
pub fn hand_lock_to(paths: &Paths, pid: u32, now: f64) {
    if let Err(error) = json_write(&paths.app_file(LOCK_FILE), &json!({"pid": pid, "zeit": now})) {
        errors::record("update_run.hand_lock_to", &error);
    }
}

pub fn release_lock(paths: &Paths) {
    remove(&paths.app_file(LOCK_FILE));
}

/// Does somebody alive hold the lock?
pub fn lock_held(paths: &Paths, now: f64) -> bool {
    let path = paths.app_file(LOCK_FILE);
    Path::new(&path).exists() && !orphaned(&path, now)
}

// ------------------------------------------------------------ run mark

/// What an update run is about to do (the fields of `update-lauf.json`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunMark {
    /// The version installed (as the release names it, `v5.0.1`).
    pub target: String,
    /// The version running before.
    pub previous: String,
    /// The installer that runs.
    pub installer: String,
    pub checksum: String,
    pub start: f64,
    /// The update came without a click; the next start then opens no main window.
    pub automatic: bool,
    /// The automatic update came right after the program was started; the next start opens the main window.
    pub show_window: bool,
}

/// Write the run mark, before the program steps down: if anything dies afterwards, the next start knows that an
/// update had begun.
///
/// # Errors
///
/// When the mark cannot be written.
pub fn begin_run(paths: &Paths, mark: &RunMark) -> io::Result<()> {
    remove(&paths.app_file(RESULT_FILE));
    json_write(
        &paths.app_file(RUN_FILE),
        &json!({
            "ziel": mark.target, "alt": mark.previous, "installer": mark.installer, "sha256": mark.checksum,
            "start": mark.start, "automatisch": mark.automatic, "fenster": mark.show_window,
        }),
    )
}

pub fn read_run(paths: &Paths) -> Option<RunMark> {
    let data = json_read(&paths.app_file(RUN_FILE))?;
    let text = |key: &str| match data.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    Some(RunMark {
        target: text("ziel"),
        previous: text("alt"),
        installer: text("installer"),
        checksum: text("sha256"),
        // ⚠ An unreadable start counts as too old: the mark goes without a word.
        start: data.get("start").and_then(Value::as_f64).unwrap_or(-RUN_MAX_AGE * 2.0),
        automatic: data.get("automatisch").and_then(Value::as_bool).unwrap_or(false),
        show_window: data.get("fenster").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// The exit code the helper wrote down, or `None`.
pub fn read_result(paths: &Paths) -> Option<i32> {
    let bytes = fs::read(paths.app_file(RESULT_FILE)).ok()?;
    String::from_utf8_lossy(&bytes).split_whitespace().next()?.parse().ok()
}

/// Write the exit code down (the helper does).
///
/// # Errors
///
/// When the file cannot be written.
pub fn write_result(paths: &Paths, code: i32) -> io::Result<()> {
    jsonio::write_text_atomic(&paths.app_file(RESULT_FILE), &format!("{code}\r\n"), jsonio::Newline::Keep)
}

fn cleanup(paths: &Paths, run: &RunMark) {
    remove(&paths.app_file(RUN_FILE));
    remove(&paths.app_file(RESULT_FILE));
    // Only a file that is recognisably ours, never anything else.
    if !run.installer.is_empty() && is_ours(&run.installer) {
        remove(&run.installer);
    }
}

/// What became of the last update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The started program has the target version.
    Done,
    /// Cancelled, the old version runs on.
    Cancelled,
    /// Failed.
    Failed,
    /// Not run to its end (no exit code).
    Unclear,
}

/// The outcome of the last update, for the start that follows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub kind: Kind,
    pub target: String,
    pub previous: String,
    pub code: Option<i32>,
    pub own: String,
    pub automatic: bool,
    pub show_window: bool,
}

impl Outcome {
    /// The sentence for the player.
    pub fn message(&self) -> String {
        let one = |key: &str, value: &str| language::tf(key, &[("0", Arg::from(value.to_string()))]);
        match self.kind {
            Kind::Done => one("up_erg_fertig", &self.target),
            Kind::Cancelled => one("up_erg_abgebrochen", &self.own),
            Kind::Unclear => one("up_erg_unklar", &self.target),
            Kind::Failed => language::tf(
                "up_erg_fehler",
                &[
                    ("0", Arg::from(self.target.clone())),
                    ("1", Arg::from(self.code.map_or_else(|| "–".to_string(), |c| c.to_string()))),
                ],
            ),
        }
    }

    /// Did the update come from the previous program (4.12)? Its jump to this version is told only with 5.1.
    pub fn from_previous_program(&self) -> bool {
        crate::version::parts(&self.previous).0 < 5
    }
}

/// At the start: what became of the last update? `None` when there is nothing to say. Cleans up afterwards.
/// ⚠ While the helper holds the lock the update runs right now: no word, and the mark stays.
pub fn evaluate(paths: &Paths, own_version: &str, now: f64) -> Option<Outcome> {
    let run = read_run(paths)?;
    if lock_held(paths, now) {
        return None;
    }
    let code = read_result(paths);
    let age = now - run.start;
    cleanup(paths, &run);
    if !(0.0..=RUN_MAX_AGE).contains(&age) {
        return None;
    }
    let own = norm(own_version);
    let kind = if !own.is_empty() && own == norm(&run.target) {
        Kind::Done
    } else if code.is_none() {
        Kind::Unclear
    } else if matches!(code, Some(2 | 5)) && own == norm(&run.previous) {
        Kind::Cancelled
    } else {
        Kind::Failed
    };
    if kind != Kind::Done {
        errors::record(
            "update_run.update",
            &format!(
                "target {}, running {}, before {}, exit code {}",
                if run.target.is_empty() { "?" } else { &run.target },
                if own_version.is_empty() { "?" } else { own_version },
                if run.previous.is_empty() { "?" } else { &run.previous },
                code.map_or_else(|| "–".to_string(), |c| c.to_string())
            ),
        );
    }
    Some(Outcome {
        kind,
        target: run.target,
        previous: run.previous,
        code,
        own: own_version.to_string(),
        automatic: run.automatic,
        show_window: run.show_window,
    })
}

/// Remember an automatic update that failed. ⚠ Without this every start would step down at once to try the
/// same failing update again.
pub fn remember_failure(paths: &Paths, outcome: &Outcome, now: f64) -> bool {
    if !outcome.automatic || outcome.kind == Kind::Done {
        return false;
    }
    remember_failed_version(paths, &outcome.target, now)
}

/// Remember that the automatic update to `version` failed (`version|time`).
pub fn remember_failed_version(paths: &Paths, version: &str, now: f64) -> bool {
    paths.set_setting(FAILED_SETTING, jsonio::Value::str(format!("{}|{}", norm(version), now as i64)))
}

/// Did the automatic update to `version` fail a short while ago?
pub fn failed_recently(paths: &Paths, version: &str, now: f64) -> bool {
    let value = paths.setting(FAILED_SETTING).unwrap_or_default();
    let (failed, stamp) = value.split_once('|').unwrap_or((value.as_str(), ""));
    if failed.is_empty() || failed != norm(version) {
        return false;
    }
    stamp.parse::<f64>().is_ok_and(|stamp| (0.0..FAILED_PAUSE).contains(&(now - stamp)))
}

/// Does the start after an update open the main window? After an update by click, after an automatic one right
/// after the start, and always when it did not work. Not after an automatic update that worked while the
/// program ran: a window opening by itself would take the focus.
pub fn should_open_window(outcome: Option<&Outcome>) -> bool {
    outcome.is_some_and(|o| !o.automatic || o.show_window || o.kind != Kind::Done)
}

/// What an update of 4.12 or of this version left in the temporary folder: the helper script of 4.12, the
/// copy of the program file that ran as the helper, and the second place of the setup log of 4.12. Run at the
/// start, after [`evaluate`]; a file that is still in use stays until the next start.
pub fn remove_leftovers(temp: &Path) {
    for name in [legacy::UPDATE_HELPER_SCRIPT, HELPER_NAME] {
        let path = temp.join(name);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
}

// ------------------------------------------------------------ the helper's log

/// Keep the last log as the one before. ⚠ Overwriting would delete exactly the failure somebody asks about.
pub fn rotate_log(paths: &Paths) {
    let previous = paths.app_file(LOG_FILE);
    if Path::new(&previous).exists()
        && let Err(error) = jsonio::os_replace(&previous, &paths.app_file(LOG_FILE_OLD))
    {
        errors::record("update_run.rotate_log", &error);
    }
}

/// One line in the helper's log: the time and the entry, in English like the installer's log next to it.
pub fn log_line(log: &Path, entry: &str) {
    use std::io::Write;
    let (year, month, day, hour, minute, second) = jsonio::local_time();
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(log) {
        let _ = write!(file, "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} {entry}\r\n");
    }
}

// ------------------------------------------------------------ handing over

/// What the helper needs to know. It travels through the environment ([`ENV_PREFIX`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HelperJob {
    /// The installer to run.
    pub setup: PathBuf,
    /// The published checksum of the installer.
    pub checksum: String,
    /// The folder the program is installed in (`/DIR`).
    pub target_dir: PathBuf,
    /// The installer's log (`/LOG`).
    pub setup_log: PathBuf,
    /// The helper's own log.
    pub log: PathBuf,
    /// Where the exit code goes.
    pub result: PathBuf,
    /// The lock the helper releases.
    pub lock: PathBuf,
    /// The program file that ran; every process of it has to end, and it is started again.
    pub program: PathBuf,
    /// The process that handed over.
    pub pid: u32,
    /// The key under the current user where the installer names its program file (it can change its name); the
    /// helper starts that one when it exists. Empty: always [`HelperJob::program`].
    pub program_key: String,
    /// How long to wait for the old program, in seconds.
    pub wait: u64,
}

const FIELDS: [&str; 11] = [
    "SETUP",
    "SHA256",
    "ZIEL",
    "SETUPLOG",
    "LOG",
    "ERGEBNIS",
    "SPERRE",
    "PROGRAMM",
    "PID",
    "PROGRAMM_SCHLUESSEL",
    "WARTEN",
];

impl HelperJob {
    /// The job as variables of the environment.
    pub fn to_env(&self) -> Vec<(String, String)> {
        let path = |p: &Path| p.to_string_lossy().into_owned();
        let values = [
            path(&self.setup),
            self.checksum.to_lowercase(),
            path(&self.target_dir),
            path(&self.setup_log),
            path(&self.log),
            path(&self.result),
            path(&self.lock),
            path(&self.program),
            self.pid.to_string(),
            self.program_key.clone(),
            self.wait.to_string(),
        ];
        FIELDS.iter().zip(values).map(|(field, value)| (format!("{ENV_PREFIX}{field}"), value)).collect()
    }

    /// The job from the environment; `None` if a part of it is missing.
    pub fn from_env(vars: &[(String, String)]) -> Option<HelperJob> {
        let get = |field: &str| {
            let name = format!("{ENV_PREFIX}{field}");
            vars.iter().find(|(k, _)| k.eq_ignore_ascii_case(&name)).map(|(_, v)| v.clone())
        };
        let path = |field: &str| get(field).filter(|v| !v.is_empty()).map(PathBuf::from);
        Some(HelperJob {
            setup: path("SETUP")?,
            checksum: get("SHA256").filter(|c| c.len() == 64)?,
            target_dir: path("ZIEL")?,
            setup_log: path("SETUPLOG")?,
            log: path("LOG")?,
            result: path("ERGEBNIS")?,
            lock: path("SPERRE")?,
            program: path("PROGRAMM")?,
            pid: get("PID")?.parse().ok()?,
            program_key: get("PROGRAMM_SCHLUESSEL").unwrap_or_default(),
            wait: get("WARTEN").and_then(|w| w.parse().ok()).unwrap_or(WAIT_SECONDS),
        })
    }
}

/// The place for the installer's log. ⚠ If the installer cannot create its log it ends at once with exit code
/// 1 and installs nothing: an old file that cannot be overwritten would stop every update. Then the log goes
/// to the temporary folder.
pub fn free_setup_log(path: &Path, temp: &Path) -> PathBuf {
    let candidates = [path.to_path_buf(), temp.join("versekit-update-setup.txt")];
    for candidate in &candidates {
        let text = candidate.to_string_lossy();
        if !candidate.exists() {
            return candidate.clone();
        }
        jsonio::make_overwritable(&text);
        if fs::remove_file(candidate).is_ok() {
            return candidate.clone();
        }
    }
    temp.join(format!("versekit-update-setup-{}.txt", updater::now() as i64))
}

/// The job for the helper of this run: the installer in `downloaded`, the program file `program` and its
/// folder.
pub fn job_for(paths: &Paths, downloaded: &updater::Downloaded, program: &Path, temp: &Path) -> HelperJob {
    HelperJob {
        setup: downloaded.path.clone(),
        checksum: downloaded.checksum.clone(),
        target_dir: program.parent().map(Path::to_path_buf).unwrap_or_default(),
        setup_log: free_setup_log(Path::new(&paths.app_file(SETUP_LOG)), temp),
        log: PathBuf::from(paths.app_file(LOG_FILE)),
        result: PathBuf::from(paths.app_file(RESULT_FILE)),
        lock: PathBuf::from(paths.app_file(LOCK_FILE)),
        program: program.to_path_buf(),
        pid: std::process::id(),
        program_key: PROGRAM_KEY.to_string(),
        wait: WAIT_SECONDS,
    }
}

/// The key under the current user where the installer names the program file (`Programm`).
pub const PROGRAM_KEY: &str = r"Software\Xharig\VerseKit";
/// The value in [`PROGRAM_KEY`].
pub const PROGRAM_VALUE: &str = "Programm";

/// Start the helper: a copy of `program` in `temp` with [`HELPER_SWITCH`] and the job in its environment, after
/// writing the run mark. The lock goes over to the helper. Returns the process number of the helper.
///
/// # Errors
///
/// When the copy or the start fails; then nothing is handed over.
pub fn start_helper(
    paths: &Paths,
    job: &HelperJob,
    mark: &RunMark,
    temp: &Path,
    base_env: Vec<(String, String)>,
) -> io::Result<u32> {
    let helper = temp.join(HELPER_NAME);
    let _ = fs::remove_file(&helper);
    fs::copy(&job.program, &helper)?;
    begin_run(paths, mark)?;
    rotate_log(paths);
    log_line(
        &job.log,
        &format!(
            "Program hands over: waiting for {} (PID {}), installer {}",
            job.program.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            job.pid,
            job.setup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        ),
    );
    let mut command = std::process::Command::new(&helper);
    command
        .arg(HELPER_SWITCH)
        .current_dir(temp)
        .env_clear()
        .envs(updater::clean_environment(base_env))
        .envs(job.to_env());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(HELPER_FLAGS);
    }
    let child = command.spawn()?;
    hand_lock_to(paths, child.id(), updater::now());
    Ok(child.id())
}

/// How the helper and the programs it starts are started: without a console window of their own, in a
/// process group of their own, apart from the program that steps down.
#[cfg(windows)]
pub const HELPER_FLAGS: u32 = windows_sys::Win32::System::Threading::CREATE_NO_WINDOW
    | windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

#[cfg(test)]
mod tests;
