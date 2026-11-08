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
//! the result the installer writes, and starting the installer under Windows.
//!
//! The file names and fields are those of 4.12: the next start of this version reads what the helper of 4.12
//! (and of 5.2.2 and earlier) left behind after the jump to it.
//!
//! * **Run mark** (`update-lauf.json`), written before the program steps down. The next start says what became
//!   of the update; ⚠ the version of the started program decides, not the exit code of the installer.
//! * **Lock** (`update-sperre.json`), created only if it is not there. ⚠ Failing to create it does not stop the
//!   update: the lock guards against a double run, the checksum against a foreign file.
//! * **Installer**: started directly by the program, with [`UPDATE_SWITCH`] and the files it reports to
//!   ([`setup_arguments`]); no copy of the program file is made and nothing is started from the temporary
//!   folder. The installer (`packaging/installer.iss`) waits until the processes named to it have ended, closes
//!   what is left, installs, writes its log lines and the exit code, removes the lock and starts the program
//!   again, also after a cancelled or failed installation.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
/// The copy of the program file that ran as the helper of 5.2.2 and earlier, in the temporary folder; only
/// removed ([`remove_leftovers`]).
pub const HELPER_NAME: &str = "versekit-update-helfer.exe";
/// The beginning of the variables the helper of 5.2.2 and earlier got its job through; they never reach a
/// program started after an update (`updater::clean_environment`).
pub const ENV_PREFIX: &str = "VERSEKIT_UPDATE_";
/// The switch that tells the installer it runs for an update of the program (`packaging/installer.iss`).
pub const UPDATE_SWITCH: &str = "/VKUPDATE";
/// The line the installer writes into the update log as soon as its own process runs (`InitializeSetup` in
/// `packaging/installer.iss`). ⚠ Only that process installs and starts the program again; the loader the
/// program starts unpacks it into the temporary folder, where a virus scanner can still stop it.
pub const LIFE_SIGN: &str = "Installer started";
/// How long the program waits for [`LIFE_SIGN`] before it gives up and stays.
pub const LIFE_SIGN_WAIT: Duration = Duration::from_secs(30);
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

/// The lock belongs to the installer from now on: the program steps down in a moment.
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

/// The exit code the installer (or the helper of 5.2.2 and earlier) wrote down, or `None`.
pub fn read_result(paths: &Paths) -> Option<i32> {
    let bytes = fs::read(paths.app_file(RESULT_FILE)).ok()?;
    String::from_utf8_lossy(&bytes).split_whitespace().next()?.parse().ok()
}

/// Write the exit code down (as the installer does).
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
/// ⚠ While the installer holds the lock the update runs right now: no word, and the mark stays.
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

/// What an update of 4.12 or of 5.2.2 and earlier left in the temporary folder: the helper script of 4.12 and
/// the copy of the program file that ran as the helper. Run at the start, after [`evaluate`]; a file that is
/// still in use stays until the next start.
pub fn remove_leftovers(temp: &Path) {
    for name in [legacy::UPDATE_HELPER_SCRIPT, HELPER_NAME] {
        let path = temp.join(name);
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
}

// ------------------------------------------------------------ the update log (`update-helfer.txt`)

/// Keep the last log as the one before. ⚠ Overwriting would delete exactly the failure somebody asks about.
pub fn rotate_log(paths: &Paths) {
    let previous = paths.app_file(LOG_FILE);
    if Path::new(&previous).exists()
        && let Err(error) = jsonio::os_replace(&previous, &paths.app_file(LOG_FILE_OLD))
    {
        errors::record("update_run.rotate_log", &error);
    }
}

/// One line in the update log: the time and the entry, in English like the installer's log next to it. The
/// installer appends its lines in the same form.
pub fn log_line(log: &Path, entry: &str) {
    use std::io::Write;
    let (year, month, day, hour, minute, second) = jsonio::local_time();
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(log) {
        let _ = write!(file, "{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} {entry}\r\n");
    }
}

// ------------------------------------------------------------ starting the installer

/// What the installer of an update is told on its command line ([`setup_arguments`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetupJob {
    /// The installer to start, where it was fetched to.
    pub setup: PathBuf,
    /// The published checksum of the installer.
    pub checksum: String,
    /// The folder the program is installed in (`/DIR`).
    pub target_dir: PathBuf,
    /// The installer's own log (`/LOG`).
    pub setup_log: PathBuf,
    /// The update log the program and the installer write lines to (`/VKPROTOKOLL`).
    pub log: PathBuf,
    /// Where the installer writes its exit code (`/VKERGEBNIS`).
    pub result: PathBuf,
    /// The lock the installer removes before it starts the program again (`/VKSPERRE`).
    pub lock: PathBuf,
    /// The processes of the program the installer waits for before it installs (`/VKPIDS`), this one first.
    pub wait_for: Vec<u32>,
}

/// The command line of the installer: silent with its progress window, no message boxes, no restart of the
/// computer, running programs closed, into the folder of the running program (⚠ else a second copy), with its
/// log; then [`UPDATE_SWITCH`] and what the update run needs. ⚠ Every path in quotes: the installer reads
/// `/NAME="a b"` as one value.
pub fn setup_arguments(job: &SetupJob) -> String {
    let pids = job.wait_for.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    format!(
        "/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR=\"{}\" /LOG=\"{}\" {UPDATE_SWITCH} \
         /VKPIDS={pids} /VKSPERRE=\"{}\" /VKERGEBNIS=\"{}\" /VKPROTOKOLL=\"{}\"",
        job.target_dir.display(),
        job.setup_log.display(),
        job.lock.display(),
        job.result.display(),
        job.log.display()
    )
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

/// The job for the installer of this run: the installer in `downloaded`, the program file `program` and its
/// folder. The installer waits for this process and every other process running `program`.
pub fn setup_job(paths: &Paths, downloaded: &updater::Downloaded, program: &Path, temp: &Path) -> SetupJob {
    let own = std::process::id();
    let mut wait_for = vec![own];
    wait_for.extend(processes_of(program).unwrap_or_default().into_iter().filter(|pid| *pid != own));
    SetupJob {
        setup: downloaded.path.clone(),
        checksum: downloaded.checksum.clone(),
        target_dir: program.parent().map(Path::to_path_buf).unwrap_or_default(),
        setup_log: free_setup_log(Path::new(&paths.app_file(SETUP_LOG)), temp),
        log: PathBuf::from(paths.app_file(LOG_FILE)),
        result: PathBuf::from(paths.app_file(RESULT_FILE)),
        lock: PathBuf::from(paths.app_file(LOCK_FILE)),
        wait_for,
    }
}

/// Start the installer of `job` directly, after writing the run mark, and hand the lock over to it once its own
/// process has written [`LIFE_SIGN`] (at most `wait`). ⚠ The checksum is compared right before the start,
/// through a handle that keeps others from changing or deleting the file until that sign has come; only a file
/// that no longer matches is deleted. Returns the process number of the installer.
///
/// # Errors
///
/// When the file no longer matches or cannot be read, the installer does not start, ends before its sign, or
/// gives no sign within `wait` (then it is ended). The run mark is gone again, the installer file is deleted
/// (not when it could only not be read), and the update log says why: the program stays.
pub fn start_setup(
    paths: &Paths,
    job: &SetupJob,
    mark: &RunMark,
    base_env: Vec<(String, String)>,
    wait: Duration,
) -> io::Result<u32> {
    launch(paths, job, mark, base_env, wait, &setup_arguments(job))
}

/// [`start_setup`] with the command line `arguments`.
fn launch(
    paths: &Paths,
    job: &SetupJob,
    mark: &RunMark,
    base_env: Vec<(String, String)>,
    wait: Duration,
    arguments: &str,
) -> io::Result<u32> {
    rotate_log(paths);
    let name = job.setup.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let held = match hold_checked_retrying(&job.setup, &job.checksum) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            log_line(&job.log, &format!("Checksum mismatch in {name} - file discarded, nothing installed: {error}"));
            log_line(&job.log, &format!("Expected {}", job.checksum));
            updater::discard(&job.setup);
            return Err(error);
        }
        Err(error) => {
            log_line(&job.log, &format!("Installer {name} could not be read - nothing installed: {error}"));
            return Err(error);
        }
    };
    begin_run(paths, mark)?;
    let pids = job.wait_for.iter().map(u32::to_string).collect::<Vec<_>>().join(", ");
    log_line(&job.log, &format!("Checksum matches, program starts the installer {name}, which waits for PID {pids}"));
    let offset = fs::metadata(&job.log).map_or(0, |m| m.len());
    let mut command = std::process::Command::new(&job.setup);
    command.env_clear().envs(updater::clean_environment(base_env));
    if let Some(folder) = job.setup.parent() {
        command.current_dir(folder);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.raw_arg(arguments).creation_flags(DETACHED_FLAGS);
    }
    #[cfg(not(windows))]
    {
        command.args(arguments.split(' '));
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            drop(held);
            give_up(paths, job, &format!("Installer did not start: {error}"));
            return Err(error);
        }
    };
    match await_life_sign(&mut child, &job.log, offset, wait) {
        Sign::Started => {}
        Sign::Ended(code) => {
            drop(held);
            give_up(paths, job, &format!("Installer ended before it ran, exit code {code} - nothing installed"));
            return Err(io::Error::other(format!("the installer ended at once with exit code {code}")));
        }
        Sign::Silent => {
            let _ = child.kill();
            let _ = child.wait();
            drop(held);
            let seconds = wait.as_secs();
            give_up(paths, job, &format!("Installer gave no sign within {seconds} s - stopped, nothing installed"));
            return Err(io::Error::other(format!("the installer gave no sign within {seconds} s")));
        }
    }
    hand_lock_to(paths, child.id(), updater::now());
    log_line(&job.log, &format!("Installer runs (PID {}), the program exits", child.id()));
    drop(held);
    Ok(child.id())
}

/// What became of the installer while the program waited for [`LIFE_SIGN`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sign {
    /// Its sign is in the log and the loader still runs.
    Started,
    /// The loader ended (with this exit code); its installer cannot take over any more.
    Ended(i32),
    /// No sign within the time.
    Silent,
}

/// Waits at most `wait` for [`LIFE_SIGN`] in `log` after `offset`, while `child` (the loader) runs.
fn await_life_sign(child: &mut std::process::Child, log: &Path, offset: u64, wait: Duration) -> Sign {
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Sign::Ended(status.code().unwrap_or(-1));
        }
        if signed(log, offset) {
            return Sign::Started;
        }
        if started.elapsed() >= wait {
            return Sign::Silent;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Is there a line ending in [`LIFE_SIGN`] in `log` after `offset`?
fn signed(log: &Path, offset: u64) -> bool {
    let Ok(bytes) = fs::read(log) else { return false };
    let tail = bytes.get(usize::try_from(offset).unwrap_or(usize::MAX)..).unwrap_or_default();
    let sign = format!(" {LIFE_SIGN}");
    String::from_utf8_lossy(tail).lines().any(|line| line.trim_end().ends_with(&sign))
}

/// [`hold_checked`], tried again a few times when the file cannot be opened or read (a scanner looking at it);
/// never again after a checksum that does not match or a file that is gone.
fn hold_checked_retrying(path: &Path, checksum: &str) -> io::Result<fs::File> {
    let mut tries = 0;
    loop {
        match hold_checked(path, checksum) {
            Err(error)
                if tries < 5
                    && !matches!(error.kind(), io::ErrorKind::InvalidData | io::ErrorKind::NotFound) =>
            {
                tries += 1;
                std::thread::sleep(Duration::from_millis(200));
            }
            other => return other,
        }
    }
}

/// The file at `path` opened for reading, with its checksum compared through that handle. Under Windows nobody
/// else may write to it or delete it while the handle lives.
fn hold_checked(path: &Path, checksum: &str) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(FILE_SHARE_READ);
    }
    let mut file = options.open(path)?;
    let actual = updater::checksum_of_reader(&mut file)?;
    if checksum.is_empty() || actual != checksum.to_lowercase() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, format!("checksum {actual}")));
    }
    Ok(file)
}

/// Others may only read a file opened with this sharing mode (`FILE_SHARE_READ`).
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 1;

/// The installer did not take over: the line in the update log, the run mark and the installer file go. The file
/// gets a few more tries: a loader that has just ended may still hold it for a moment; what stays goes at the
/// next start ([`remove_fetched`]).
fn give_up(paths: &Paths, job: &SetupJob, why: &str) {
    log_line(&job.log, why);
    remove(&paths.app_file(RUN_FILE));
    for _ in 0..20 {
        match fs::remove_file(&job.setup) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => std::thread::sleep(Duration::from_millis(100)),
            _ => return,
        }
    }
    updater::discard(&job.setup);
}

/// Removes what earlier updates fetched into `folder` ([`updater::installer_folder`]): every file but `keep`
/// (the installer of a run under way) and those changed within [`LOCK_MAX_AGE`] (a download of another run may
/// be coming in). A file still in use stays for the next start.
pub fn remove_fetched(folder: &Path, keep: Option<&Path>, now: f64) {
    let Ok(entries) = fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || keep.is_some_and(|keep| same_path(keep, &path)) {
            continue;
        }
        let age = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(f64::INFINITY, |m| now - m.as_secs_f64());
        if age > LOCK_MAX_AGE {
            let _ = fs::remove_file(&path);
        }
    }
}

/// How the installer is started: without a console window of its own, in a process group of its own, apart
/// from the program that steps down.
#[cfg(windows)]
pub const DETACHED_FLAGS: u32 = windows_sys::Win32::System::Threading::CREATE_NO_WINDOW
    | windows_sys::Win32::System::Threading::CREATE_NEW_PROCESS_GROUP;

// ------------------------------------------------------------ processes of the program file

/// The processes running exactly this program file, without this one; `None` when the list cannot be read
/// (and outside Windows).
pub fn processes_of(program: &Path) -> Option<Vec<u32>> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
        };
        let wanted = program.file_name()?.to_string_lossy().to_lowercase();
        let own = std::process::id();
        // SAFETY: a snapshot handle that is closed below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        // SAFETY: an all-zero entry is a valid empty one; its size is set as the call wants.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = Vec::new();
        // SAFETY: `entry` carries its size; the snapshot is valid until closed.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase();
            if name == wanted
                && entry.th32ProcessID != own
                && image_of(entry.th32ProcessID).is_none_or(|image| same_path(&image, program))
            {
                found.push(entry.th32ProcessID);
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        // SAFETY: the handle from above, closed once.
        unsafe { CloseHandle(snapshot) };
        Some(found)
    }
    #[cfg(not(windows))]
    {
        let _ = program;
        None
    }
}

/// The program file of a process (Windows); `None` when it cannot be asked (then a process of the same name
/// counts as ours: the installer waiting once too long is better than one that finds the file in use).
#[cfg(windows)]
fn image_of(pid: u32) -> Option<PathBuf> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    // SAFETY: a handle that is closed below.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; 32_768];
    let mut size = buffer.len() as u32;
    // SAFETY: the buffer and its size in characters.
    let read = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &raw mut size) } != 0;
    // SAFETY: the handle from above, closed once.
    unsafe { CloseHandle(handle) };
    read.then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..size as usize])))
}

/// Are two paths the same place (without regard to case under Windows)?
pub fn same_path(a: &Path, b: &Path) -> bool {
    let full = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (a, b) = (full(a), full(b));
    if cfg!(windows) { a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase() } else { a == b }
}

#[cfg(test)]
mod tests;
