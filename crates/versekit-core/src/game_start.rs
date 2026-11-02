// Verse-Kit — companion for Star Citizen.
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

//! Starting the game: where the starter is (`paths.game_starter`), how the entered
//! command is split (`_start_command`), the start itself (`start_game`) and the
//! MangoHud switch (`mangohud.py`).
//!
//! ⚠ The start goes through [`Spawner`]: the self-checks and the tests put a
//! [`Recorder`] in its place, so no launcher and no game is ever started by them.
//!
//! The start programs of 4.12 (`start_programs`: a program that replaces the RSI
//! Launcher or runs with it) are not part of this module; they belong to the
//! package for the settings of the game.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::language::t;
use crate::paths::{Paths, is_file};
use crate::pypath::Flavor;

/// The setting with the own start command (Lutris, Heroic, Flatpak or a script).
pub const STARTER_SETTING: &str = "spielstarter";

/// The setting that switches MangoHud on for the start (Linux only).
pub const MANGOHUD_SETTING: &str = "mangohud";

/// The name of the script the LUG Helper puts into the Wine prefix.
const LINUX_SCRIPT: &str = "sc-launch.sh";

/// Where the package manager puts the Vulkan layer of MangoHud.
const LAYER_DIRS: [&str; 6] = [
    "/usr/share/vulkan/implicit_layer.d",
    "/usr/local/share/vulkan/implicit_layer.d",
    "/etc/vulkan/implicit_layer.d",
    "/etc/xdg/vulkan/implicit_layer.d",
    "~/.local/share/vulkan/implicit_layer.d",
    "~/.config/vulkan/implicit_layer.d",
];

/// What the game is started with, or `None`: the entered command, else the launcher
/// of Windows or the script of the LUG Helper.
///
/// A start command entered by hand counts even when no such file exists (it may be a
/// command with arguments). Nothing found: no button, a button that does nothing is
/// worse than none.
pub fn game_starter(paths: &Paths) -> Option<String> {
    // `Paths::setting` already strips the text and gives nothing for an empty one, as 4.12 does.
    if let Some(custom) = paths.setting(STARTER_SETTING) {
        return Some(custom);
    }
    let f = paths.host.flavor;
    let game = paths.game_folder();
    if f == Flavor::Windows {
        let mut places: Vec<String> = Vec::new();
        // Next to the game folder first: …\Roberts Space Industries\StarCitizen\LIVE is the game,
        // …\Roberts Space Industries\RSI Launcher the launcher. Fixed places miss other drives.
        if let Some(game) = &game {
            let rsi = f.dirname(&f.dirname(game));
            places.push(f.join_all(&rsi, &["RSI Launcher", "RSI Launcher.exe"]));
            places.push(f.join_all(&f.dirname(&rsi), &["RSI Launcher", "RSI Launcher.exe"]));
        }
        // What Windows knows beats every list.
        if let Some(found) = paths.launcher_from_registry() {
            places.push(found);
        }
        for name in ["LOCALAPPDATA", "PROGRAMFILES", "PROGRAMW6432"] {
            let Some(root) = paths.host.var(name).filter(|r| !r.is_empty()) else { continue };
            places.push(f.join_all(root, &["Programs", "RSI Launcher", "RSI Launcher.exe"]));
            places.push(f.join_all(root, &["RSI Launcher", "RSI Launcher.exe"]));
            places.push(f.join_all(root, &["Roberts Space Industries", "RSI Launcher", "RSI Launcher.exe"]));
        }
        return places.into_iter().find(|p| is_file(p));
    }
    // The LUG Helper starts nothing itself: the script it makes in the prefix does. The prefix lies
    // above `drive_c`, so the way goes from the game folder up to it.
    let mut places: Vec<String> = Vec::new();
    if let Some(game) = &game {
        let mut path = absolute(game);
        loop {
            let parent = f.dirname(&path);
            if parent == path {
                break;
            }
            if f.basename(&path).to_lowercase() == "drive_c" {
                places.push(f.join(&parent, LINUX_SCRIPT));
                break;
            }
            path = parent;
        }
    }
    // The usual place of the LUG Helper, in case the game folder is not known (yet).
    places.push(f.join_all(&paths.host.home(), &["Games", "star-citizen", LINUX_SCRIPT]));
    // No fall back to the helper itself: it would be found, the button would show, and do nothing.
    places.into_iter().find(|p| is_file(p) && is_executable(p))
}

/// `os.path.abspath` of an existing or planned path: made absolute against the working folder.
fn absolute(path: &str) -> String {
    if Path::new(path).is_absolute() {
        return path.to_string();
    }
    std::env::current_dir().map_or_else(|_| path.to_string(), |dir| dir.join(path).to_string_lossy().into_owned())
}

#[cfg(unix)]
fn is_executable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &str) -> bool {
    true
}

/// The list `Command` needs from the entered text.
///
/// ⚠ A start command is not always a file name: `flatpak run org.starcitizen-lug.Helper` or
/// `lutris rungame/star-citizen` are commands with arguments, split as a shell would split them
/// (with quotes for paths with blanks). A file that exists is NOT split, or
/// `/home/me/My Games/sc-launch.sh` would fall into three parts.
pub fn start_command(starter: &str, flavor: Flavor) -> Vec<String> {
    if Path::new(starter).exists() {
        return vec![starter.to_string()];
    }
    match shlex_split(starter, flavor != Flavor::Windows) {
        Some(parts) if !parts.is_empty() => parts,
        // An unpaired quote or the like: the text stays one piece.
        _ => vec![starter.to_string()],
    }
}

/// Python's `shlex.split(text, posix=posix)` (without comments). `None` for a quote that is not closed
/// or an escape at the end (Python raises `ValueError`).
///
/// ⚠ Without `posix` (Windows) the quotes stay in the pieces and a piece that begins with a quote ends
/// at the closing one, as in Python.
pub fn shlex_split(text: &str, posix: bool) -> Option<Vec<String>> {
    #[derive(PartialEq)]
    enum State {
        Between,
        Word,
        Single,
        Double,
    }
    let blank = |c: char| matches!(c, ' ' | '\t' | '\r' | '\n');
    let mut out = Vec::new();
    let mut token = String::new();
    let mut state = State::Between;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match state {
            State::Between | State::Word => {
                if blank(c) {
                    if state == State::Word {
                        out.push(std::mem::take(&mut token));
                    }
                    state = State::Between;
                } else if c == '\'' || c == '"' {
                    if posix {
                        // An empty pair of quotes still makes a piece: the state is a word.
                        state = if c == '\'' { State::Single } else { State::Double };
                    } else if state == State::Between {
                        token.push(c);
                        state = if c == '\'' { State::Single } else { State::Double };
                    } else {
                        token.push(c);
                    }
                } else if c == '\\' && posix {
                    token.push(chars.next()?);
                    state = State::Word;
                } else {
                    token.push(c);
                    state = State::Word;
                }
            }
            State::Single | State::Double => {
                let closing = if state == State::Single { '\'' } else { '"' };
                if c == closing {
                    if posix {
                        state = State::Word;
                    } else {
                        // Without posix the piece ends at the closing quote.
                        token.push(c);
                        out.push(std::mem::take(&mut token));
                        state = State::Between;
                    }
                } else if c == '\\' && posix && state == State::Double {
                    let next = chars.next()?;
                    if next != '"' && next != '\\' {
                        token.push('\\');
                    }
                    token.push(next);
                } else {
                    token.push(c);
                }
            }
        }
    }
    match state {
        State::Single | State::Double => return None,
        State::Word => out.push(token),
        State::Between => {}
    }
    Some(out)
}

/// One started process, as the recorder writes it down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    pub command: Vec<String>,
    /// Variables put on top of the inherited environment.
    pub env: Vec<(String, String)>,
}

/// What starts a process: the real one, or a recorder that starts nothing.
pub trait Spawner: Send + Sync {
    /// Starts `command` detached from this program (it must not hang on the game and must not take
    /// the game down with it when this program ends).
    fn spawn(&self, command: &[String], env: &[(String, String)]) -> Result<(), String>;

    /// Starts a program of the player (start programs) in `cwd` and keeps its handle: it is ended
    /// again when the game ends, if the player asked for that.
    ///
    /// A spawner that knows no programs of the player says so (the default).
    fn spawn_program(&self, _command: &[String], _cwd: Option<&str>) -> Result<Box<dyn Running>, String> {
        Err("no programs".to_string())
    }
}

/// A program started by [`Spawner::spawn_program`].
pub trait Running: Send {
    /// Does it run still?
    fn is_running(&mut self) -> bool;
    /// Asks it to end.
    fn terminate(&mut self);
}

struct Child(std::process::Child);

impl Running for Child {
    fn is_running(&mut self) -> bool {
        matches!(self.0.try_wait(), Ok(None))
    }

    /// Where the system knows it (Linux, macOS), a request to end first (`SIGTERM`, like `terminate()` of
    /// 4.12), so that a program can save what it holds; it is made to end after three seconds. Windows
    /// has no such request for a program without a window of its own: it is ended at once.
    fn terminate(&mut self) {
        #[cfg(unix)]
        {
            if let Ok(pid) = i32::try_from(self.0.id()) {
                // SAFETY: `kill` with a process id of a child we still hold and a plain signal number.
                unsafe { libc::kill(pid, libc::SIGTERM) };
                for _ in 0..30 {
                    if !matches!(self.0.try_wait(), Ok(None)) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
        let _ = self.0.kill();
        // Collected, so that it does not stay behind as a zombie.
        let _ = self.0.wait();
    }
}

/// What a recorder saw of a program of the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartedProgram {
    pub command: Vec<String>,
    pub cwd: Option<String>,
}

/// The handle the recorder hands out: it counts how often the program was asked to end.
struct Recorded(std::sync::Arc<std::sync::atomic::AtomicUsize>);

impl Running for Recorded {
    fn is_running(&mut self) -> bool {
        true
    }

    fn terminate(&mut self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// The real start.
pub struct System;

impl Spawner for System {
    fn spawn(&self, command: &[String], env: &[(String, String)]) -> Result<(), String> {
        let (program, args) = command.split_first().ok_or_else(|| "empty command".to_string())?;
        let mut run = std::process::Command::new(program);
        run.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        for (name, value) in env {
            run.env(name, value);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            run.creation_flags(DETACHED_PROCESS);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Its own group, away from this program (4.12: a new session).
            run.process_group(0);
        }
        run.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    fn spawn_program(&self, command: &[String], cwd: Option<&str>) -> Result<Box<dyn Running>, String> {
        let (program, args) = command.split_first().ok_or_else(|| "empty command".to_string())?;
        let mut run = std::process::Command::new(program);
        run.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(cwd) = cwd.filter(|c| !c.is_empty()) {
            run.current_dir(cwd);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            run.creation_flags(DETACHED_PROCESS);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            run.process_group(0);
        }
        run.spawn().map(|child| Box::new(Child(child)) as Box<dyn Running>).map_err(|e| e.to_string())
    }
}

/// The self-check's and the tests' stand-in: starts nothing, writes down what would have started.
#[derive(Default)]
pub struct Recorder {
    pub started: Mutex<Vec<Started>>,
    /// The programs of the player (start programs) that would have started.
    pub programs: Mutex<Vec<StartedProgram>>,
    /// How often one of them was asked to end.
    pub ended: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Recorder {
    pub fn calls(&self) -> Vec<Started> {
        self.started.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn program_calls(&self) -> Vec<StartedProgram> {
        self.programs.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Spawner for Recorder {
    fn spawn(&self, command: &[String], env: &[(String, String)]) -> Result<(), String> {
        self.started
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Started { command: command.to_vec(), env: env.to_vec() });
        Ok(())
    }

    fn spawn_program(&self, command: &[String], cwd: Option<&str>) -> Result<Box<dyn Running>, String> {
        self.programs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(StartedProgram { command: command.to_vec(), cwd: cwd.map(str::to_string) });
        Ok(Box::new(Recorded(self.ended.clone())))
    }
}

/// Is there a way to start the game: a starter, or a start program that takes the place of the RSI Launcher
/// (4.12: `_has_starter` of the main window)? The button, the rocket and the entry of the menu depend on it.
pub fn can_start(paths: &Paths) -> bool {
    game_starter(paths).is_some() || crate::start_programs::replacement(paths).is_some()
}

/// Starts Star Citizen. `Err(reason)` says why not (shown through `s_sp_start_nein`).
///
/// A start program that replaces the RSI Launcher takes its place (and nothing else starts); otherwise the
/// starter is run and the start programs that run with the launcher follow.
///
/// ⚠ A failure of the MangoHud part never stops the start: it goes without.
pub fn start_game(paths: &Paths, spawner: &std::sync::Arc<dyn Spawner>) -> Result<(), String> {
    if let Some(instead) = crate::start_programs::replacement(paths) {
        return crate::start_programs::launch(&instead, spawner.clone(), paths.host.flavor, false)
            .map_err(|error| error.reason());
    }
    let Some(starter) = game_starter(paths) else { return Err(t("s_sp_kein_starter")) };
    let possible = mangohud_possible(paths);
    let env = mangohud_env(possible, possible && mangohud_wanted(paths), possible && mangohud_installed(paths));
    spawner.spawn(&start_command(&starter, paths.host.flavor), &env)?;
    crate::start_programs::on_launcher(paths, spawner);
    Ok(())
}

/// The variables MangoHud needs for the start: `MANGOHUD=1` only when it is possible, wanted and installed.
///
/// ⚠ Only `MANGOHUD=1`, never `MANGOHUD_CONFIG`: with that set MangoHud no longer reads the player's own file.
pub fn mangohud_env(possible: bool, wanted: bool, installed: bool) -> Vec<(String, String)> {
    if possible && wanted && installed { vec![("MANGOHUD".to_string(), "1".to_string())] } else { Vec::new() }
}

/// What is known about the game running, for everything that deletes or writes in files of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    /// The Game.log was written a moment ago.
    Running,
    /// The Game.log is old: the game does not run.
    Idle,
    /// There is no Game.log to ask (no game folder known): nobody can say.
    Unknown,
}

impl RunState {
    /// Nothing is deleted or written unless the game certainly does not run: unknown counts as running
    /// (decided by the main session, 2026-10-22; 4.12 said "in doubt yes" in the text and the opposite in the code).
    pub fn blocks_writing(self) -> bool {
        self != RunState::Idle
    }
}

/// A made-up machine of a test that must not look at the real process list sets this variable.
pub const ENV_NO_PROCESS: &str = "SC_BP_NO_PROCESS";

/// Is the game in the process list? `None` when the list cannot be read (and on systems where this
/// program does not know how to read it).
pub fn game_process() -> Option<bool> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
        };
        // SAFETY: a snapshot handle that is closed below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        // SAFETY: an all-zero entry is a valid empty one; its size is set as the call wants.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        // SAFETY: `entry` carries its size; the snapshot is valid until closed.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case("starcitizen.exe") {
                found = true;
                break;
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        // SAFETY: the handle from above, closed once.
        unsafe { CloseHandle(snapshot) };
        Some(found)
    }
    #[cfg(target_os = "linux")]
    {
        // The game runs under Wine: its processes carry `StarCitizen.exe` in their command line.
        let entries = std::fs::read_dir("/proc").ok()?;
        for entry in entries.flatten() {
            let name = entry.file_name();
            if !name.to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if let Ok(line) = std::fs::read(entry.path().join("cmdline"))
                && String::from_utf8_lossy(&line).to_lowercase().contains("starcitizen.exe")
            {
                return Some(true);
            }
        }
        Some(false)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

/// Seconds since a Game.log was written, or `None` without one.
fn age_of(path: &str) -> Option<f64> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    Some(std::time::SystemTime::now().duration_since(modified).map_or(0.0, |d| d.as_secs_f64()))
}

/// Does the game run? The process list first, then the Game.log of **every** channel that is installed
/// (the game may run in another channel than the folder the program reads), none of them written in the last
/// `mission_log::GAME_IDLE_SEC`. Deleting asks this, and an error costs there: whatever cannot be told counts as
/// running (`Unknown`).
pub fn run_state(paths: &Paths) -> RunState {
    let process = if paths.host.var(ENV_NO_PROCESS).is_some() { None } else { game_process() };
    run_state_with(paths, process)
}

/// [`run_state`] with the answer of the process list given (the tests give it).
pub fn run_state_with(paths: &Paths, process: Option<bool>) -> RunState {
    let mut ages: Vec<f64> = crate::overlay::log_age(paths).into_iter().collect();
    for (_, folder, _) in paths.installed_channels() {
        ages.extend(paths.game_log(Some(&folder)).and_then(|log| age_of(&log)));
    }
    // In the process list, or a log written lately in any channel: it runs.
    if process == Some(true) || ages.iter().any(|age| *age < crate::mission_log::GAME_IDLE_SEC) {
        return RunState::Running;
    }
    // Nothing written lately and not in the list: the list can tell it is not there; without it, a log that
    // lies quiet is the word, and no log at all is nothing to go by.
    match (process, ages.is_empty()) {
        (None, true) => RunState::Unknown,
        _ => RunState::Idle,
    }
}

/// The channels of the game that are test builds: what they hold is confidential by the terms of service.
const TEST_CHANNELS: [&str; 3] = ["PTU", "EPTU", "TECH-PREVIEW"];

/// The name of the test channel (`PTU`, `EPTU`, `TECH-PREVIEW`) the game folder is, or `None` for LIVE,
/// HOTFIX and everything else. The folder is named like its channel.
///
/// ⚠ 4.12 shows nothing about this; the hint is asked for by the community (greluc): whoever reads data of a
/// test build must know that the terms of service keep it confidential.
pub fn test_channel(folder: &str) -> Option<String> {
    let name = folder.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next()?.to_uppercase();
    TEST_CHANNELS.contains(&name.as_str()).then_some(name)
}

/// What a window knows about starting the game, looked up in a thread of its own and renewed now and
/// then: the lookup touches the file system (and on Windows the registry), and a drawing thread
/// must not wait for that.
#[derive(Default)]
pub struct Look {
    found: std::sync::Arc<Mutex<Found>>,
    busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    last: Option<std::time::Instant>,
}

/// The result of one lookup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    /// A way to start the game is there ([`game_starter`]).
    pub starter: bool,
    /// MangoHud is set up on this system ([`mangohud_installed`]).
    pub mangohud: bool,
    /// The first lookup is done.
    pub looked: bool,
}

/// How long a result of [`Look`] stands before the next lookup.
pub const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(20);

impl Look {
    /// The last result; starts a new lookup when the last is older than [`LOOK_EVERY`] (or `renew`)
    /// and none is running. Never waits.
    pub fn poll(&mut self, paths: &Paths, renew: bool) -> Found {
        use std::sync::atomic::Ordering;
        let stale = renew || self.last.is_none_or(|at| at.elapsed() >= LOOK_EVERY);
        if stale && !self.busy.swap(true, Ordering::AcqRel) {
            self.last = Some(std::time::Instant::now());
            let (found, busy, copy) = (self.found.clone(), self.busy.clone(), paths.working_copy());
            std::thread::spawn(move || {
                let now = Found { starter: can_start(&copy), mangohud: mangohud_installed(&copy), looked: true };
                *found.lock().unwrap_or_else(|e| e.into_inner()) = now;
                busy.store(false, Ordering::Release);
            });
        }
        self.found.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// Does MangoHud run on this system at all? Only on Linux.
pub fn mangohud_possible(paths: &Paths) -> bool {
    cfg!(target_os = "linux") && paths.host.flavor != Flavor::Windows
}

/// Did the player switch it on?
pub fn mangohud_wanted(paths: &Paths) -> bool {
    paths.setting_bool(MANGOHUD_SETTING, false)
}

/// Is MangoHud set up on this system: a layer file in one of the folders, or the program `mangohud` on the path.
pub fn mangohud_installed(paths: &Paths) -> bool {
    if !mangohud_possible(paths) {
        return false;
    }
    let folders: Vec<PathBuf> = LAYER_DIRS.iter().map(|d| PathBuf::from(paths.host.expanduser(d))).collect();
    let path = paths.host.var("PATH").unwrap_or_default().to_string();
    layer_in(&folders) || on_path("mangohud", &path)
}

/// Is there a file `*ango*ud*.json` in one of the folders?
pub fn layer_in(folders: &[PathBuf]) -> bool {
    let matches = |name: &str| {
        name.find("ango")
            .and_then(|at| name[at + 4..].find("ud").map(|to| &name[at + 4 + to + 2..]))
            .is_some_and(|rest| rest.ends_with(".json"))
    };
    folders.iter().any(|dir| {
        std::fs::read_dir(dir).is_ok_and(|entries| entries.flatten().any(|e| matches(&e.file_name().to_string_lossy())))
    })
}

/// `shutil.which`: the program in one of the folders of the path (separated as the system does).
pub fn on_path(program: &str, path: &str) -> bool {
    std::env::split_paths(path).any(|dir| {
        let candidate = dir.join(program);
        let text = candidate.to_string_lossy();
        is_file(&text) && is_executable(&text)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonio::Value;
    use crate::paths::{ENV_INSTALL_DIR, Host};

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-start-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().into_owned()
    }

    fn touch(path: &str, text: &str) {
        std::fs::create_dir_all(Path::new(path).parent().expect("parent")).expect("folder");
        std::fs::write(path, text).expect("file");
    }

    /// A script the system may run (on Windows every file counts).
    fn script(path: &str) {
        touch(path, "#!/bin/sh\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("mode");
        }
    }

    /// The paths of a Linux user: `vars` besides HOME.
    fn linux(home: &str, vars: &[(&str, &str)]) -> Paths {
        let mut all = vec![("HOME", home)];
        all.extend_from_slice(vars);
        Paths::new(Host::new(Flavor::Posix, &all, home))
    }

    fn split(text: &str, posix: bool) -> Option<Vec<String>> {
        shlex_split(text, posix)
    }

    fn pieces(parts: &[&str]) -> Option<Vec<String>> {
        Some(parts.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn a_command_is_split_as_a_shell_does() {
        assert_eq!(
            split("flatpak run org.starcitizen-lug.Helper", true),
            pieces(&["flatpak", "run", "org.starcitizen-lug.Helper"])
        );
        assert_eq!(split("lutris rungame/star-citizen", true), pieces(&["lutris", "rungame/star-citizen"]));
        // Quotes keep a path with blanks together and are gone from the piece.
        assert_eq!(
            split("'/home/me/My Games/sc-launch.sh' --go", true),
            pieces(&["/home/me/My Games/sc-launch.sh", "--go"])
        );
        assert_eq!(split("run \"a b\" c\\ d", true), pieces(&["run", "a b", "c d"]));
        // Inside double quotes a backslash escapes only a quote and itself.
        assert_eq!(split(r#""a\"b" "c\nd""#, true), pieces(&["a\"b", "c\\nd"]));
        // An empty pair of quotes is an empty piece.
        assert_eq!(split("a '' b", true), pieces(&["a", "", "b"]));
        assert_eq!(split("", true), pieces(&[]));
        assert_eq!(split("   ", true), pieces(&[]));
    }

    #[test]
    fn a_quote_that_is_not_closed_or_an_escape_at_the_end_is_an_error() {
        assert_eq!(split("run 'a b", true), None);
        assert_eq!(split("run \"a b", true), None);
        assert_eq!(split("run a\\", true), None);
        assert_eq!(split("run \"a\\", true), None);
        assert_eq!(split("run 'a b", false), None);
    }

    #[test]
    fn without_posix_the_quotes_stay_and_a_quoted_piece_ends_at_its_closing_quote() {
        // The way Windows gets it from Python: the backslashes are no escapes, the quotes stay in the piece.
        assert_eq!(split(r#""C:\Program Files\x.exe" --a"#, false), pieces(&[r#""C:\Program Files\x.exe""#, "--a"]));
        assert_eq!(split(r#""a"b"#, false), pieces(&[r#""a""#, "b"]));
        // A quote inside a word is an ordinary character.
        assert_eq!(split("it's fine", false), pieces(&["it's", "fine"]));
        assert_eq!(split(r"C:\x\y.exe /a", false), pieces(&[r"C:\x\y.exe", "/a"]));
    }

    #[test]
    fn a_file_that_exists_is_not_split_and_an_unsplittable_text_stays_whole() {
        let dir = std::env::temp_dir().join(format!("versekit-start-{}-split", crate::testdir::run_tag()));
        std::fs::create_dir_all(&dir).expect("folder");
        let script = dir.join("My Games sc-launch.sh");
        std::fs::write(&script, "x").expect("file");
        let text = script.to_string_lossy().into_owned();
        assert_eq!(start_command(&text, Flavor::Posix), vec![text.clone()]);
        assert_eq!(
            start_command("lutris rungame/x", Flavor::Posix),
            vec!["lutris".to_string(), "rungame/x".to_string()]
        );
        assert_eq!(start_command("run 'a", Flavor::Posix), vec!["run 'a".to_string()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The prefix of the LUG Helper with the game in it; the script lies next to `drive_c`.
    fn prefix(home: &str, folder: &str) -> (String, String) {
        let root = format!("{home}/{folder}");
        let live = format!("{root}/drive_c/Program Files/Roberts Space Industries/StarCitizen/LIVE");
        touch(&format!("{live}/Game.log"), "x");
        (root, live)
    }

    #[test]
    fn on_linux_the_script_next_to_drive_c_is_found_from_the_game_folder() {
        let home = scratch("linux-game");
        let (root, live) = prefix(&home, "anywhere/prefix");
        script(&format!("{root}/{LINUX_SCRIPT}"));
        let paths = linux(&home, &[(ENV_INSTALL_DIR, &live)]);
        assert_eq!(game_starter(&paths), Some(format!("{root}/{LINUX_SCRIPT}")), "wherever the prefix lies");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn on_linux_the_usual_place_is_the_fall_back_and_the_helper_itself_is_none() {
        let home = scratch("linux-usual");
        // No game known, only the helper: no starter, no button.
        script(&format!("{home}/bin/lug-helper"));
        let paths = linux(&home, &[]);
        assert_eq!(game_starter(&paths), None);
        script(&format!("{home}/Games/star-citizen/{LINUX_SCRIPT}"));
        assert_eq!(game_starter(&paths), Some(format!("{home}/Games/star-citizen/{LINUX_SCRIPT}")));
        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(unix)]
    #[test]
    fn on_linux_a_script_that_cannot_run_is_no_starter() {
        let home = scratch("linux-mode");
        touch(&format!("{home}/Games/star-citizen/{LINUX_SCRIPT}"), "x");
        assert_eq!(game_starter(&linux(&home, &[])), None);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_command_entered_by_hand_beats_everything_found() {
        let home = scratch("custom");
        script(&format!("{home}/Games/star-citizen/{LINUX_SCRIPT}"));
        let paths = linux(&home, &[]);
        assert!(paths.set_setting(STARTER_SETTING, Value::str("  lutris rungame/star-citizen  ")));
        assert_eq!(game_starter(&paths), Some("lutris rungame/star-citizen".to_string()), "trimmed, no file needed");
        // Emptied again: the way that was found counts again.
        assert!(paths.set_setting(STARTER_SETTING, Value::str("")));
        assert_eq!(game_starter(&paths), Some(format!("{home}/Games/star-citizen/{LINUX_SCRIPT}")));
        let _ = std::fs::remove_dir_all(home);
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_the_launcher_next_to_the_game_folder_comes_before_the_fixed_places() {
        let base = scratch("win");
        let rsi = format!("{base}\\Roberts Space Industries");
        touch(&format!("{rsi}\\StarCitizen\\LIVE\\Game.log"), "x");
        let fixed = format!("{base}\\local");
        touch(&format!("{fixed}\\Programs\\RSI Launcher\\RSI Launcher.exe"), "x");
        let live = format!("{rsi}\\StarCitizen\\LIVE");
        let host = |vars: &[(&str, &str)]| {
            let mut all = vec![(ENV_INSTALL_DIR, live.as_str()), ("LOCALAPPDATA", fixed.as_str())];
            all.extend_from_slice(vars);
            let mut host = Host::new(Flavor::Windows, &all, &base);
            host.registry = false;
            Paths::new(host)
        };
        // Only the fixed place has a launcher: found there.
        assert_eq!(game_starter(&host(&[])), Some(format!("{fixed}\\Programs\\RSI Launcher\\RSI Launcher.exe")));
        // Next to the game folder one is found first.
        touch(&format!("{rsi}\\RSI Launcher\\RSI Launcher.exe"), "x");
        assert_eq!(game_starter(&host(&[])), Some(format!("{rsi}\\RSI Launcher\\RSI Launcher.exe")));
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn nothing_found_is_a_reason_and_nothing_started() {
        let home = scratch("none");
        let recorder = std::sync::Arc::new(Recorder::default());
        let spawner: std::sync::Arc<dyn Spawner> = recorder.clone();
        let paths = linux(&home, &[]);
        assert_eq!(start_game(&paths, &spawner), Err(t("s_sp_kein_starter")));
        assert!(recorder.calls().is_empty(), "nothing was started");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_start_hands_the_split_command_to_the_spawner() {
        let home = scratch("start");
        let recorder = std::sync::Arc::new(Recorder::default());
        let spawner: std::sync::Arc<dyn Spawner> = recorder.clone();
        let paths = linux(&home, &[]);
        assert!(paths.set_setting(STARTER_SETTING, Value::str("lutris rungame/star-citizen")));
        assert_eq!(start_game(&paths, &spawner), Ok(()));
        assert_eq!(
            recorder.calls(),
            [Started { command: vec!["lutris".into(), "rungame/star-citizen".into()], env: Vec::new() }]
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn start_programs_go_with_the_launcher_or_take_its_place() {
        use crate::start_programs::{self, Entry, When};
        let home = scratch("programme");
        let recorder = std::sync::Arc::new(Recorder::default());
        let spawner: std::sync::Arc<dyn Spawner> = recorder.clone();
        let paths = linux(&home, &[]);
        let tool = format!("{home}/tool.exe");
        std::fs::write(&tool, "x").expect("file");
        let entry = |when| Entry { name: "Tool".into(), file: tool.clone(), when, ..Entry::default() };
        // Nothing found, but a program that replaces the launcher: there is a way to start, and it is that program.
        assert!(!can_start(&paths));
        assert!(start_programs::save(&paths, &[entry(When::Replaces)]));
        assert!(can_start(&paths));
        assert_eq!(start_game(&paths, &spawner), Ok(()));
        assert!(recorder.calls().is_empty(), "the launcher itself is not started");
        assert_eq!(recorder.program_calls().len(), 1);
        // A program with the launcher follows the launcher.
        assert!(paths.set_setting(STARTER_SETTING, Value::str("lutris rungame/x")));
        assert!(start_programs::save(&paths, &[entry(When::Launcher)]));
        assert_eq!(start_game(&paths, &spawner), Ok(()));
        for _ in 0..40 {
            if recorder.program_calls().len() == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert_eq!(recorder.calls().len(), 1, "the launcher started");
        assert_eq!(recorder.program_calls().len(), 2, "and the program with it");
        // A program that replaces the launcher whose file is gone: the reason is the file.
        std::fs::remove_file(&tool).expect("remove");
        assert!(start_programs::save(&paths, &[entry(When::Replaces)]));
        let reason = start_game(&paths, &spawner).expect_err("the file is gone");
        assert!(reason.contains(&tool) && !reason.contains('—'), "the reason names the file: {reason}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_spawner_that_fails_gives_its_reason_back() {
        struct Broken;
        impl Spawner for Broken {
            fn spawn(&self, _: &[String], _: &[(String, String)]) -> Result<(), String> {
                Err("kaputt".into())
            }
        }
        let home = scratch("broken");
        let paths = linux(&home, &[]);
        assert!(paths.set_setting(STARTER_SETTING, Value::str("x")));
        let broken: std::sync::Arc<dyn Spawner> = std::sync::Arc::new(Broken);
        assert_eq!(start_game(&paths, &broken), Err("kaputt".to_string()));
        let _ = std::fs::remove_dir_all(home);
    }

    /// Polls until the lookup has run (a thread of its own), at most three seconds.
    fn looked(look: &mut Look, paths: &Paths, renew: bool, wanted: bool) -> Found {
        let mut found = look.poll(paths, renew);
        for _ in 0..60 {
            if found.looked && found.starter == wanted {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            found = look.poll(paths, false);
        }
        found
    }

    #[test]
    fn the_look_finds_the_way_in_a_thread_of_its_own_and_renews_on_demand() {
        let home = scratch("look");
        let paths = linux(&home, &[]);
        let mut look = Look::default();
        // The first call never waits for the lookup: nothing is known yet.
        assert_eq!(look.poll(&paths, false), Found::default());
        let found = looked(&mut look, &paths, false, false);
        assert!(found.looked && !found.starter, "no way yet: {found:?}");
        // An own command is entered: not seen before the lookup is renewed (it is run every twenty seconds).
        assert!(paths.set_setting(STARTER_SETTING, Value::str("lutris rungame/x")));
        assert!(!look.poll(&paths, false).starter, "a result stands until it is renewed");
        assert!(looked(&mut look, &paths, true, true).starter, "renewed: the command counts");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn mangohud_comes_with_the_start_only_when_possible_wanted_and_installed() {
        let on = vec![("MANGOHUD".to_string(), "1".to_string())];
        for possible in [false, true] {
            for wanted in [false, true] {
                for installed in [false, true] {
                    let expected = if possible && wanted && installed { on.clone() } else { Vec::new() };
                    assert_eq!(mangohud_env(possible, wanted, installed), expected, "{possible} {wanted} {installed}");
                }
            }
        }
    }

    #[test]
    fn unknown_counts_as_running_for_everything_that_writes_in_the_files_of_the_game() {
        let home = scratch("laeuft");
        let game = format!("{home}/StarCitizen/LIVE");
        touch(&format!("{game}/Game.log"), "x");
        let paths = linux(&home, &[(ENV_INSTALL_DIR, game.as_str()), (ENV_NO_PROCESS, "1")]);
        // A Game.log written a moment ago: the game runs.
        assert_eq!(run_state(&paths), RunState::Running);
        // An old one: it does not.
        age(&format!("{game}/Game.log"), 3600);
        assert_eq!(run_state(&paths), RunState::Idle);
        assert!(!run_state(&paths).blocks_writing());
        // No game folder: nobody can say, so nothing is written.
        let nothing = scratch("laeuft-nichts");
        let none = linux(&nothing, &[(ENV_NO_PROCESS, "1")]);
        assert_eq!(run_state(&none), RunState::Unknown);
        assert!(RunState::Unknown.blocks_writing() && RunState::Running.blocks_writing());
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(nothing);
    }

    fn age(path: &str, seconds: u64) {
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds);
        std::fs::File::options().write(true).open(path).and_then(|f| f.set_modified(old)).expect("age");
    }

    #[test]
    fn the_process_list_and_every_channel_decide_before_anything_is_deleted() {
        let home = scratch("laeuft-prozess");
        let live = format!("{home}/StarCitizen/LIVE");
        let ptu = format!("{home}/StarCitizen/PTU");
        touch(&format!("{live}/Game.log"), "x");
        touch(&format!("{ptu}/Game.log"), "x");
        age(&format!("{live}/Game.log"), 3600);
        let paths = linux(&home, &[(ENV_INSTALL_DIR, live.as_str()), (ENV_NO_PROCESS, "1")]);
        // The folder that is read lies quiet, the game is played in the other channel: it runs.
        assert_eq!(run_state_with(&paths, None), RunState::Running, "a fresh log of another channel counts");
        age(&format!("{ptu}/Game.log"), 3600);
        assert_eq!(run_state_with(&paths, None), RunState::Idle, "all quiet");
        // The process list is the word where there is one.
        assert_eq!(run_state_with(&paths, Some(true)), RunState::Running, "in the list, whatever the logs say");
        assert_eq!(run_state_with(&paths, Some(false)), RunState::Idle);
        // No log at all: the list says it does not run, or nobody can say.
        let nothing = scratch("laeuft-prozess-leer");
        let none = linux(&nothing, &[(ENV_NO_PROCESS, "1")]);
        assert_eq!(run_state_with(&none, Some(false)), RunState::Idle);
        assert_eq!(run_state_with(&none, Some(true)), RunState::Running);
        assert_eq!(run_state_with(&none, None), RunState::Unknown);
        let _ = std::fs::remove_dir_all(home);
        let _ = std::fs::remove_dir_all(nothing);
    }

    #[test]
    fn the_test_channels_are_known_by_the_name_of_their_folder() {
        for (folder, channel) in [
            ("C:\\Games\\Roberts Space Industries\\StarCitizen\\PTU", Some("PTU")),
            ("/home/me/Games/star-citizen/drive_c/RSI/StarCitizen/eptu/", Some("EPTU")),
            ("D:\\RSI\\StarCitizen\\TECH-PREVIEW", Some("TECH-PREVIEW")),
            ("C:\\RSI\\StarCitizen\\LIVE", None),
            ("C:\\RSI\\StarCitizen\\HOTFIX", None),
            ("C:\\RSI\\PTU\\StarCitizen", None),
            ("", None),
        ] {
            assert_eq!(test_channel(folder).as_deref(), channel, "{folder}");
        }
    }

    #[test]
    fn the_layer_file_of_mangohud_is_found_by_its_name() {
        let dir = std::env::temp_dir().join(format!("versekit-start-{}-layer", crate::testdir::run_tag()));
        std::fs::create_dir_all(&dir).expect("folder");
        assert!(!layer_in(std::slice::from_ref(&dir)), "an empty folder has none");
        std::fs::write(dir.join("other.json"), "x").expect("file");
        std::fs::write(dir.join("MangoHud.x86_64.txt"), "x").expect("file");
        assert!(!layer_in(std::slice::from_ref(&dir)), "neither the wrong name nor the wrong ending");
        std::fs::write(dir.join("MangoHud.x86_64.json"), "x").expect("file");
        assert!(layer_in(std::slice::from_ref(&dir)));
        assert!(!layer_in(&[dir.join("not-there")]));
        let _ = std::fs::remove_dir_all(dir);
    }
}
