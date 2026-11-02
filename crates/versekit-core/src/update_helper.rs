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

//! The update helper under Windows: a copy of the program file in the temporary folder, so that the installer
//! can replace the real one (4.12 used a script there).
//!
//! In this order: wait until every process of the program file has ended (overlay, main window, the window for
//! teaching the digits; at most [`HelperJob::wait`] seconds, else [`RC_OLD_STUCK`]); compare the installer
//! with the published checksum once more, right before it starts (else it is deleted, [`RC_CHECKSUM_BAD`]);
//! run the installer silently into the folder of the program; write the exit code down; release the lock; and
//! start the program again after the codes in [`RESTART_AFTER`]. The steps that touch the system go through
//! [`System`], so that a check can stand in for them.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::update_run::{HelperJob, PROGRAM_VALUE, RC_CHECKSUM_BAD, RC_OLD_STUCK, RESTART_AFTER, log_line};
use crate::updater::still_matches;

/// What the helper does to the system.
pub trait System {
    /// The processes running `program` (the same file), `None` when the list cannot be read.
    fn processes_of(&self, program: &Path) -> Option<Vec<u32>>;
    /// Does this process live?
    fn alive(&self, pid: u32) -> bool;
    fn sleep(&self, time: Duration);
    /// Run the installer with these arguments (written as they are) and wait for its exit code.
    ///
    /// # Errors
    ///
    /// When it cannot be started.
    fn run_setup(&self, setup: &Path, arguments: &str) -> io::Result<i32>;
    /// The program file the installer names under `key`, if any.
    fn installed_program(&self, key: &str) -> Option<PathBuf>;
    /// Start the program, apart from the helper.
    ///
    /// # Errors
    ///
    /// When it cannot be started.
    fn start(&self, program: &Path) -> io::Result<()>;
}

/// The arguments of the installer: silent, no message boxes, no restart of the computer, the running program
/// closed, into the folder of the running program (⚠ else a second copy), with a log.
pub fn setup_arguments(job: &HelperJob) -> String {
    format!(
        "/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR=\"{}\" /LOG=\"{}\"",
        job.target_dir.display(),
        job.setup_log.display()
    )
}

/// Wait until the program that handed over and every process of its file have ended. True when they have.
fn old_version_gone(job: &HelperJob, system: &dyn System) -> bool {
    let rounds = job.wait.max(1);
    for round in 0..=rounds {
        let handing_over = system.alive(job.pid);
        let others = system.processes_of(&job.program).unwrap_or_default();
        if !handing_over && others.is_empty() {
            return true;
        }
        if round < rounds {
            system.sleep(Duration::from_secs(1));
        }
    }
    false
}

/// The whole run of the helper. Returns the exit code it wrote down.
pub fn run(job: &HelperJob, system: &dyn System) -> i32 {
    log_line(&job.log, "Helper started");
    let code = if !old_version_gone(job, system) {
        log_line(&job.log, "The old version does not exit - nothing installed");
        RC_OLD_STUCK
    } else if !still_matches(&job.setup, &job.checksum) {
        log_line(&job.log, "Old version has exited");
        log_line(&job.log, "Checksum mismatch - file discarded, nothing installed");
        log_line(&job.log, &format!("Expected {}", job.checksum));
        let _ = std::fs::remove_file(&job.setup);
        RC_CHECKSUM_BAD
    } else {
        log_line(&job.log, "Old version has exited");
        log_line(&job.log, "Checksum matches, starting installer");
        let code = match system.run_setup(&job.setup, &setup_arguments(job)) {
            Ok(code) => code,
            Err(error) => {
                log_line(&job.log, &format!("Installer did not start: {error}"));
                -1
            }
        };
        log_line(&job.log, &format!("Installer finished, exit code {code}"));
        code
    };
    if let Err(error) = crate::jsonio::write_text_atomic(
        &job.result.to_string_lossy(),
        &format!("{code}\r\n"),
        crate::jsonio::Newline::Keep,
    ) {
        log_line(&job.log, &format!("Exit code not written: {error}"));
    }
    let _ = std::fs::remove_file(&job.lock);
    if RESTART_AFTER.contains(&code) {
        let program = restart_program(job, system);
        log_line(&job.log, &format!("Starting the program {}", program.display()));
        if let Err(error) = system.start(&program) {
            log_line(&job.log, &format!("Start failed: {error}"));
        }
    } else {
        log_line(&job.log, &format!("No restart after exit code {code}"));
    }
    code
}

/// The program to start: the one the installer names, if it is there and lies in the folder installed into;
/// else the one that ran.
pub fn restart_program(job: &HelperJob, system: &dyn System) -> PathBuf {
    if !job.program_key.is_empty()
        && let Some(named) = system.installed_program(&job.program_key)
        && named.is_file()
        && named.parent().is_some_and(|folder| same_path(folder, &job.target_dir))
    {
        return named;
    }
    job.program.clone()
}

/// Are two paths the same place (without regard to case under Windows)?
pub fn same_path(a: &Path, b: &Path) -> bool {
    let full = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (a, b) = (full(a), full(b));
    if cfg!(windows) { a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase() } else { a == b }
}

/// The real system.
pub struct Real {
    /// The environment for the installer and the program started again (cleaned, without the helper's job).
    pub env: Vec<(String, String)>,
}

impl System for Real {
    fn processes_of(&self, program: &Path) -> Option<Vec<u32>> {
        processes_of(program)
    }

    fn alive(&self, pid: u32) -> bool {
        crate::update_run::pid_alive(i64::from(pid))
    }

    fn sleep(&self, time: Duration) {
        std::thread::sleep(time);
    }

    fn run_setup(&self, setup: &Path, arguments: &str) -> io::Result<i32> {
        let mut command = std::process::Command::new(setup);
        command.env_clear().envs(self.env.iter().cloned());
        if let Some(folder) = setup.parent() {
            command.current_dir(folder);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.raw_arg(arguments).creation_flags(crate::update_run::HELPER_FLAGS);
        }
        #[cfg(not(windows))]
        {
            command.args(arguments.split(' '));
        }
        Ok(command.status()?.code().unwrap_or(-1))
    }

    fn installed_program(&self, key: &str) -> Option<PathBuf> {
        #[cfg(windows)]
        {
            use crate::paths::registry;
            registry::open(registry::HKEY_CURRENT_USER, key)?.value(PROGRAM_VALUE).map(PathBuf::from)
        }
        #[cfg(not(windows))]
        {
            let _ = (key, PROGRAM_VALUE);
            None
        }
    }

    fn start(&self, program: &Path) -> io::Result<()> {
        let mut command = std::process::Command::new(program);
        command.env_clear().envs(self.env.iter().cloned());
        if let Some(folder) = program.parent() {
            command.current_dir(folder);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(crate::update_run::HELPER_FLAGS);
        }
        command.spawn().map(|_| ())
    }
}

/// The processes running exactly this program file, `None` when the list cannot be read (and outside Windows,
/// where the helper does not run).
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
/// counts as ours: waiting once too long is better than an installer that finds the file in use).
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

#[cfg(test)]
mod tests;
