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

//! What the RSI launcher's log (`%APPDATA%\rsilauncher\logs\log.log`) tells about the patches it installs: whether it
//! is patching a `Data.p4k` right now, and the name of the build it installed.
//!
//! The launcher first writes the new structure of the archive and then fills in the files; an archive read in between
//! lists new entries whose data are not there yet. A delta update runs from its line `Starting delta update (SC
//! <channel> <build>) in <install folder>` to `Delta update completed (SC <channel> <build>) in <channel folder>`; an
//! error of the pipeline ends it as well. A start without an end counts as running only while the launcher still
//! writes its log ([`LAUNCHER_ALIVE`]): a launcher closed in the middle of a patch does not lock the archive for ever.
//!
//! The launcher names the build by its patch (`4.10.3-live.13165946`); the `build_manifest.id` of the game names the
//! branch it was cut from, which may be an older one (`sc-alpha-4.10.0` for the same change number).
//!
//! The log lies below `APPDATA` under Windows and in the Wine prefix of the game under Linux ([`log_folders`]); the
//! launcher moves it to [`OLD_LOG_FILE`] when it starts a new one.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::paths::Paths;

/// The log file below the configuration folder of Windows (`APPDATA`).
pub const LOG_FILE: [&str; 3] = ["rsilauncher", "logs", "log.log"];

/// How long after its last line the launcher still counts as working on a patch it started. It writes a line at
/// least once a minute while it is open.
pub const LAUNCHER_ALIVE: Duration = Duration::from_secs(20 * 60);

/// One delta update of a channel, as the launcher names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    /// The channel as the launcher writes it: `LIVE`, `PTU`, `EPTU`, `TECH-PREVIEW`, …
    pub channel: String,
    /// The build, e.g. `4.10.3-live.13165946`.
    pub build: String,
    /// The folder the line names: the install folder for a start, the channel folder for an end.
    pub folder: String,
}

/// A line of the log that starts or ends a delta update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Started(Update),
    Completed(Update),
    /// An error of the pipeline, or a delta update that failed: every update that ran has ended.
    Failed,
}

/// The `Update` in `(SC <channel> <build>) in <folder>` after `marker`, with the backslashes of the JSON line undone.
fn update_after(line: &str, marker: &str) -> Option<Update> {
    let rest = &line[line.find(marker)? + marker.len()..];
    let (inside, after) = rest.split_once(')')?;
    let (channel, build) = inside.trim().split_once(' ')?;
    let folder = after.strip_prefix(" in ")?;
    let end = [" (statistics", "\""].iter().filter_map(|stop| folder.find(stop)).min().unwrap_or(folder.len());
    let folder = folder[..end].trim().replace("\\\\", "\\");
    Some(Update { channel: channel.trim().to_string(), build: build.trim().to_string(), folder })
}

/// The starts and ends of delta updates in the log `text`, in its order.
pub fn events(text: &str) -> Vec<Event> {
    const START: &str = "[PatcherPhase] Starting delta update (SC ";
    const DONE: &str = "[PatcherPhase] Delta update completed (SC ";
    text.lines()
        .filter_map(|line| {
            if line.contains(START) {
                update_after(line, START).map(Event::Started)
            } else if line.contains(DONE) {
                update_after(line, DONE).map(Event::Completed)
            } else if line.contains("[Pipeline] Error")
                || (line.contains("[PatcherPhase]") && (line.contains("failed") || line.contains("cancel")))
            {
                Some(Event::Failed)
            } else {
                None
            }
        })
        .collect()
}

/// A folder as text to compare: lower case, one kind of separator, no separator at the end.
fn folder_key(folder: &str) -> String {
    folder.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// The channel folder of `archive` (`…\LIVE`) and the install folder above it (`…\StarCitizen`).
fn folders_of(archive: &Path) -> Option<(String, String)> {
    let channel = archive.parent()?;
    let install = channel.parent()?;
    Some((folder_key(&channel.to_string_lossy()), folder_key(&install.to_string_lossy())))
}

/// Whether `events` end with a delta update into the folder of `archive` that has not ended: started for the install
/// folder above it and the channel its folder is named after, with no end of that channel and no failure after it.
pub fn open_update(events: &[Event], archive: &Path) -> bool {
    let Some((channel_folder, install)) = folders_of(archive) else { return false };
    let channel_name = channel_folder.rsplit('\\').next().unwrap_or_default().to_string();
    let mut open = false;
    for event in events {
        match event {
            Event::Started(update) => {
                if folder_key(&update.folder) == install && update.channel.to_lowercase() == channel_name {
                    open = true;
                }
            }
            Event::Completed(update) => {
                if folder_key(&update.folder) == channel_folder {
                    open = false;
                }
            }
            Event::Failed => open = false,
        }
    }
    open
}

/// The name the launcher gives the build with the change number `change` in `channel` (as scmdb writes it: `live`,
/// `ptu`, …), the last one it names; `None` when it names none.
pub fn build_name(events: &[Event], channel: &str, change: &str) -> Option<String> {
    if change.is_empty() || channel.is_empty() {
        return None;
    }
    let end = format!("-{}.{change}", channel.to_lowercase());
    events
        .iter()
        .rev()
        .filter_map(|event| match event {
            Event::Started(update) | Event::Completed(update) => Some(&update.build),
            Event::Failed => None,
        })
        .find(|build| build.to_lowercase().ends_with(&end) && build.len() > end.len())
        .cloned()
}

/// The file the launcher moves its log to when it starts a new one, beside [`LOG_FILE`].
pub const OLD_LOG_FILE: &str = "log.old.log";

/// The folders the launcher's log may lie in for `archive`: below `APPDATA` (Windows), and in the Wine prefix the
/// archive lies in (Linux: `<prefix>/drive_c/users/<user>/AppData/Roaming/rsilauncher/logs`, the prefix found by
/// going up from the archive to `drive_c`). Only folders that exist, each once.
pub fn log_folders(paths: &Paths, archive: Option<&Path>) -> Vec<PathBuf> {
    let logs = |roaming: PathBuf| LOG_FILE[..2].iter().fold(roaming, |path, part| path.join(part));
    let mut found: Vec<PathBuf> = Vec::new();
    if let Some(base) = paths.host.var_set("APPDATA") {
        found.push(logs(PathBuf::from(base)));
    }
    let drive_c = archive.and_then(|archive| {
        archive.ancestors().find(|folder| folder.file_name().is_some_and(|name| name.eq_ignore_ascii_case("drive_c")))
    });
    if let Some(users) = drive_c.map(|drive| drive.join("users"))
        && let Ok(entries) = std::fs::read_dir(&users)
    {
        let mut users: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).collect();
        users.sort();
        found.extend(users.into_iter().map(|user| logs(user.join("AppData").join("Roaming"))));
    }
    let mut kept: Vec<PathBuf> = Vec::new();
    for folder in found {
        if folder.is_dir() && !kept.contains(&folder) {
            kept.push(folder);
        }
    }
    kept
}

/// The launcher's current log for `archive` ([`log_folders`]); `None` where there is none.
pub fn log_file(paths: &Paths, archive: Option<&Path>) -> Option<PathBuf> {
    log_folders(paths, archive).into_iter().map(|folder| folder.join(LOG_FILE[2])).find(|file| file.is_file())
}

/// The events of a log file, read again only when the file changed.
fn read_events(file: &Path) -> Option<(Vec<Event>, Option<SystemTime>)> {
    type Kept = Vec<(PathBuf, u64, Option<SystemTime>, Vec<Event>)>;
    static KEPT: Mutex<Kept> = Mutex::new(Vec::new());
    let meta = std::fs::metadata(file).ok()?;
    let (len, modified) = (meta.len(), meta.modified().ok());
    let mut kept = KEPT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, _, _, events)) = kept.iter().find(|(path, l, m, _)| path == file && *l == len && *m == modified) {
        return Some((events.clone(), modified));
    }
    let bytes = std::fs::read(file).ok()?;
    let found = events(&String::from_utf8_lossy(&bytes));
    kept.retain(|(path, ..)| path != file);
    kept.push((file.to_path_buf(), len, modified, found.clone()));
    Some((found, modified))
}

/// Whether the launcher is patching `archive` right now ([`open_update`]), as its current log tells; false without
/// a log, and once the log was not written for [`LAUNCHER_ALIVE`].
pub fn patching(paths: &Paths, archive: &Path) -> bool {
    let Some(file) = log_file(paths, Some(archive)) else { return false };
    let Some((events, modified)) = read_events(&file) else { return false };
    let alive =
        modified.and_then(|at| SystemTime::now().duration_since(at).ok()).is_none_or(|since| since < LAUNCHER_ALIVE);
    alive && open_update(&events, archive)
}

/// The name the launcher gives the build of `archive` with the change number `change` in `channel` ([`build_name`]):
/// the moved log first, then the current one, so that a patch whose lines went into the moved log is still found.
pub fn installed_build(paths: &Paths, archive: &Path, channel: &str, change: &str) -> Option<String> {
    let mut all: Vec<Event> = Vec::new();
    for folder in log_folders(paths, Some(archive)) {
        for name in [OLD_LOG_FILE, LOG_FILE[2]] {
            if let Some((events, _)) = read_events(&folder.join(name)) {
                all.extend(events);
            }
        }
    }
    build_name(&all, channel, change)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lines of a patch of LIVE as the launcher writes them, with the lines between left out.
    const PATCH_START: &str = r#"{ "t":"2026-11-13 13:08:08.524", "[main][info] ": "[PatcherPhase] Starting delta update (SC LIVE 4.10.3-live.13165946) in C:\\Program Files\\Roberts Space Industries\\StarCitizen (statistics enabled: false)"  },
{ "t":"2026-11-13 13:08:57.289", "[main][info] ": "[CigDataPatcher] Calling progress callback: installer@update-p4k-structure-start"  },
{ "t":"2026-11-13 13:09:01.698", "[main][info] ": "[CigDataPatcher] Calling progress callback: installer@update-files-inside-p4k-start"  },
{ "t":"2026-11-13 13:10:29.535", "[main][info] ": "[API] request: https://robertsspaceindustries.com/api/spectrum/v2/guide/stats"  },"#;
    const PATCH_END: &str = r#"{ "t":"2026-11-13 13:16:15.164", "[main][info] ": "[CigDataPatcher] Calling progress callback: installer@update-files-inside-p4k-end"  },
{ "t":"2026-11-13 13:16:15.221", "[main][info] ": "[PatcherPhase] Delta update completed (SC LIVE 4.10.3-live.13165946) in C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE"  },
{ "t":"2026-11-13 13:16:15.222", "[main][info] ": "[PatcherPhase] Transfer statistics complete."  },"#;
    const LIVE: &str = r"C:\Program Files\Roberts Space Industries\StarCitizen\LIVE\Data.p4k";

    #[test]
    fn the_lines_of_a_patch_are_read_with_channel_build_and_folder() {
        let found = events(&format!("{PATCH_START}\n{PATCH_END}"));
        assert_eq!(
            found,
            vec![
                Event::Started(Update {
                    channel: "LIVE".into(),
                    build: "4.10.3-live.13165946".into(),
                    folder: r"C:\Program Files\Roberts Space Industries\StarCitizen".into(),
                }),
                Event::Completed(Update {
                    channel: "LIVE".into(),
                    build: "4.10.3-live.13165946".into(),
                    folder: r"C:\Program Files\Roberts Space Industries\StarCitizen\LIVE".into(),
                }),
            ]
        );
    }

    #[test]
    fn an_archive_is_being_patched_from_the_start_of_its_update_until_its_end() {
        let archive = Path::new(LIVE);
        assert!(open_update(&events(PATCH_START), archive), "started, not ended: the launcher writes the archive");
        assert!(!open_update(&events(&format!("{PATCH_START}\n{PATCH_END}")), archive), "ended");
        assert!(!open_update(&events(""), archive), "no patch at all");
        // An error of the pipeline ends it as well.
        let failed = format!(
            "{PATCH_START}\n{}",
            r#"{ "t":"2026-07-20 01:21:44.703", "[main][error]": "[Pipeline] Error: BaseP4kCorruptedError: UNKNOWN ERROR"  },"#
        );
        assert!(!open_update(&events(&failed), archive));
        // A new start after an end is open again.
        assert!(open_update(&events(&format!("{PATCH_START}\n{PATCH_END}\n{PATCH_START}")), archive));
    }

    #[test]
    fn a_patch_of_another_channel_or_installation_leaves_the_archive_free() {
        let events = events(PATCH_START);
        assert!(!open_update(
            &events,
            Path::new(r"C:\Program Files\Roberts Space Industries\StarCitizen\PTU\Data.p4k")
        ));
        assert!(!open_update(&events, Path::new(r"E:\Testumgebung\Spieldaten LIVE\Data.p4k")));
        assert!(!open_update(&events, Path::new("Data.p4k")));
        // The case of the folder does not matter, nor the kind of separator.
        assert!(open_update(&events, Path::new("c:/program files/roberts space industries/starcitizen/live/Data.p4k")));
    }

    #[test]
    fn the_build_is_named_as_the_launcher_names_it_for_the_same_change_number() {
        let events = events(&format!("{PATCH_START}\n{PATCH_END}"));
        assert_eq!(build_name(&events, "live", "13165946").as_deref(), Some("4.10.3-live.13165946"));
        assert_eq!(build_name(&events, "ptu", "13165946"), None, "another channel");
        assert_eq!(build_name(&events, "live", "13165945"), None, "another build");
        assert_eq!(build_name(&events, "live", "65946"), None, "only the whole change number");
        assert_eq!(build_name(&events, "live", ""), None);
    }

    /// A machine of the tests in a fresh folder named after `name`, with `APPDATA` there when `appdata` says so.
    fn machine(name: &str, appdata: bool) -> (PathBuf, Paths) {
        let home = std::env::temp_dir().join(format!("versekit-launcher-{name}-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let text = home.to_string_lossy().into_owned();
        let mut vars = vec![("HOME", text.as_str()), ("SC_BP_HOME", text.as_str())];
        if appdata {
            vars.push(("APPDATA", text.as_str()));
        }
        let paths = Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &text));
        (home, paths)
    }

    /// The launcher moved the lines of the patch into its old log when it started a new one: the name is found there.
    #[test]
    fn the_build_name_is_found_in_the_moved_log_too() {
        let (home, paths) = machine("alt", true);
        let logs = home.join("rsilauncher").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join(OLD_LOG_FILE), format!("{PATCH_START}\n{PATCH_END}")).unwrap();
        std::fs::write(
            logs.join("log.log"),
            r#"{ "t":"2026-11-14 08:00:00.000", "[main][info] ": "[API] request"  },"#,
        )
        .unwrap();
        let archive = Path::new(LIVE);
        assert_eq!(installed_build(&paths, archive, "live", "13165946").as_deref(), Some("4.10.3-live.13165946"));
        assert!(!patching(&paths, archive), "the current log decides whether a patch runs");
        let _ = std::fs::remove_dir_all(home);
    }

    /// Under Linux the launcher runs in the Wine prefix of the game: its log is found from the archive, without
    /// `APPDATA`.
    #[test]
    fn under_wine_the_log_lies_in_the_prefix_of_the_game() {
        let (home, paths) = machine("wine", false);
        let drive_c = home.join("prefix").join("drive_c");
        let live = drive_c.join("Program Files").join("Roberts Space Industries").join("StarCitizen").join("LIVE");
        std::fs::create_dir_all(&live).unwrap();
        let logs =
            drive_c.join("users").join("spieler").join("AppData").join("Roaming").join("rsilauncher").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let install = live.parent().unwrap().to_string_lossy().replace('\\', "\\\\");
        let start = format!(
            r#"{{ "t":"2026-11-13 13:08:08.524", "[main][info] ": "[PatcherPhase] Starting delta update (SC LIVE 4.10.3-live.13165946) in {install} (statistics enabled: false)"  }},"#
        );
        std::fs::write(logs.join("log.log"), &start).unwrap();
        let archive = live.join("Data.p4k");
        assert_eq!(log_folders(&paths, Some(&archive)), [logs.clone()]);
        assert_eq!(log_file(&paths, None), None, "without the archive there is no prefix to look in");
        assert_eq!(installed_build(&paths, &archive, "live", "13165946").as_deref(), Some("4.10.3-live.13165946"));
        assert!(patching(&paths, &archive), "the patch runs in the prefix");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_log_of_this_machine_counts_only_while_the_launcher_writes_it() {
        let home = std::env::temp_dir().join(format!("versekit-launcher-log-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&home);
        let logs = home.join("rsilauncher").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let text = home.to_string_lossy().into_owned();
        let paths = Paths::new(crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[("HOME", text.as_str()), ("SC_BP_HOME", text.as_str()), ("APPDATA", text.as_str())],
            &text,
        ));
        let archive = Path::new(LIVE);
        assert!(!patching(&paths, archive), "no log: free");
        let file = logs.join("log.log");
        std::fs::write(&file, PATCH_START).unwrap();
        assert!(patching(&paths, archive), "a patch running");
        assert_eq!(installed_build(&paths, archive, "live", "13165946").as_deref(), Some("4.10.3-live.13165946"));
        // A launcher closed in the middle of the patch: its log stands still and no longer locks the archive.
        let old = SystemTime::now() - LAUNCHER_ALIVE - Duration::from_secs(60);
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(old).unwrap();
        assert!(!patching(&paths, archive), "the launcher is gone");
        std::fs::write(&file, format!("{PATCH_START}\n{PATCH_END}")).unwrap();
        assert!(!patching(&paths, archive), "the patch ended");
        let _ = std::fs::remove_dir_all(home);
    }
}
