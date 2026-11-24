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
//!
//! The log also says which installation is really started: `[Launcher::launch] Launching Star Citizen LIVE from
//! (<channel folder>)` names the folder of the channel, `[Pipeline] Installing Star Citizen LIVE <build> at <folder>
//! (type: install)` the folder above the channels (written at the first installation already, before any `Game.log`
//! exists). [`launched_folders`] gives the folder of each channel named last, [`offer`] the one the game folder set
//! should be changed to; the program never changes the setting by itself for it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use crate::paths::{CHANNELS, Paths};
use crate::pypath::Flavor;

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
    // The launcher names the folder it patches, not the link a channel folder may be to it (HOTFIX to LIVE): the
    // archive is looked for under the first channel folder that leads to the same real folder.
    let leading = archive.parent().map(|folder| {
        let first = crate::paths::leading_channel_folder(&folder.to_string_lossy());
        Path::new(&first).join(archive.file_name().unwrap_or_default())
    });
    let archive = leading.as_deref().unwrap_or(archive);
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

/// The folders the launcher's log may lie in for `archive`: below `APPDATA` of `paths` and of the machine this
/// process runs on (Windows; a copy of the data names its own `APPDATA`, while the launcher writes into the one of the
/// machine), and in the Wine prefix the archive lies in (Linux: `<prefix>/drive_c/users/<user>/AppData/Roaming/
/// rsilauncher/logs`, the prefix found by going up from the archive to `drive_c`). Only folders that exist, each once.
pub fn log_folders(paths: &Paths, archive: Option<&Path>) -> Vec<PathBuf> {
    // A host with given drives looks at nothing of this computer, not at the launcher logs of its user either.
    let machine = if paths.host.drives_given() { None } else { crate::paths::global().host.var_set("APPDATA") };
    log_folders_with(paths, archive, machine)
}

/// [`log_folders`] with the `APPDATA` of the machine given.
fn log_folders_with(paths: &Paths, archive: Option<&Path>, machine: Option<&str>) -> Vec<PathBuf> {
    let logs = |roaming: PathBuf| LOG_FILE[..2].iter().fold(roaming, |path, part| path.join(part));
    let mut found: Vec<PathBuf> = Vec::new();
    for base in paths.host.var_set("APPDATA").into_iter().chain(machine) {
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
    patching_in(&log_folders(paths, Some(archive)), archive)
}

/// [`patching`] with the log folders given.
fn patching_in(folders: &[PathBuf], archive: &Path) -> bool {
    let Some(file) = folders.iter().map(|folder| folder.join(LOG_FILE[2])).find(|file| file.is_file()) else {
        return false;
    };
    let Some((events, modified)) = read_events(&file) else { return false };
    let alive =
        modified.and_then(|at| SystemTime::now().duration_since(at).ok()).is_none_or(|since| since < LAUNCHER_ALIVE);
    alive && open_update(&events, archive)
}

/// The name the launcher gives the build of `archive` with the change number `change` in `channel` ([`build_name`]):
/// the moved log first, then the current one, so that a patch whose lines went into the moved log is still found.
pub fn installed_build(paths: &Paths, archive: &Path, channel: &str, change: &str) -> Option<String> {
    installed_build_in(&log_folders(paths, Some(archive)), channel, change)
}

/// [`installed_build`] with the log folders given.
fn installed_build_in(folders: &[PathBuf], channel: &str, change: &str) -> Option<String> {
    let mut all: Vec<Event> = Vec::new();
    for folder in folders {
        for name in [OLD_LOG_FILE, LOG_FILE[2]] {
            if let Some((events, _)) = read_events(&folder.join(name)) {
                all.extend(events);
            }
        }
    }
    build_name(&all, channel, change)
}

/// A channel folder the launcher names in a line of its log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    /// When the line was written, as the log has it (`2026-11-13 13:08:08.524`); sorts by time. Empty without one.
    pub when: String,
    /// The channel as the launcher writes it: `LIVE`, `PTU`, …
    pub channel: String,
    /// The folder of the channel as the launcher writes it (a Windows path, the backslashes of the JSON line undone).
    pub folder: String,
}

/// The time a line of the log was written (`"t":"2026-11-13 13:08:08.524"`); empty when it has none.
fn time_of(line: &str) -> String {
    let Some(at) = line.find("\"t\"") else { return String::new() };
    let rest = line[at + 3..].trim_start().trim_start_matches(':').trim_start();
    rest.strip_prefix('"').and_then(|text| text.split('"').next()).unwrap_or_default().to_string()
}

/// The channel folder one line names: the line of a launch names it (`Launching Star Citizen LIVE from (<folder>)`),
/// the line of an install or update names the folder above the channels (`Installing Star Citizen LIVE <build> at
/// <folder> (type: install)`), the channel folder lies in it.
fn named_in(line: &str) -> Option<Named> {
    const LAUNCHING: &str = "[Launcher::launch] Launching Star Citizen ";
    const INSTALLING: &str = "[Pipeline] Installing Star Citizen ";
    let (marker, launch) = if line.contains(LAUNCHING) {
        (LAUNCHING, true)
    } else if line.contains(INSTALLING) {
        (INSTALLING, false)
    } else {
        return None;
    };
    let rest = &line[line.find(marker)? + marker.len()..];
    // The text of the JSON line ends with its quote.
    let rest = rest.split('"').next().unwrap_or_default().trim();
    let (channel, rest) = rest.split_once(' ')?;
    let undo = |folder: &str| folder.trim().replace("\\\\", "\\").trim_end_matches('\\').to_string();
    let folder = if launch {
        undo(rest.strip_prefix("from (")?.trim_end().strip_suffix(')')?)
    } else {
        let (_, after) = rest.split_once(" at ")?;
        let install = undo(&after[..after.rfind(" (type:").unwrap_or(after.len())]);
        if install.is_empty() { install } else { format!("{install}\\{channel}") }
    };
    (!channel.is_empty() && !folder.is_empty()).then(|| Named {
        when: time_of(line),
        channel: channel.to_string(),
        folder,
    })
}

/// The channel folders the lines of the log name, in its order.
pub fn named_folders(text: &str) -> Vec<Named> {
    text.lines().filter_map(named_in).collect()
}

/// A Windows folder of the log as a folder of the Wine prefix `prefix`: `C:` is its `drive_c`, another drive is
/// behind its link in `dosdevices`. `None` for text that is no folder of a drive, or that leaves it.
fn wine_folder(prefix: &Path, folder: &str) -> Option<String> {
    let (drive, rest) = folder.split_once(":\\")?;
    let mut letters = drive.chars();
    let letter = letters.next()?.to_ascii_lowercase();
    if letters.next().is_some() || !letter.is_ascii_alphabetic() {
        return None;
    }
    // Written as text with `/`, as Wine's paths are (a `d:` is no component of a path to build on every system).
    let mut path = prefix.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
    path.push_str(&if letter == 'c' { "/drive_c".to_string() } else { format!("/dosdevices/{letter}:") });
    for part in rest.split('\\').filter(|part| !part.is_empty()) {
        if part == ".." || part == "." || part.contains('/') {
            return None;
        }
        path.push('/');
        path.push_str(part);
    }
    Some(path)
}

/// The folders the launcher's log may lie in: those of [`log_folders`], and (under Linux) in each Wine prefix of the
/// usual places, since the folder of the game that tells the prefix may not be known yet (a first installation).
fn log_places(paths: &Paths, near: Option<&Path>) -> Vec<PathBuf> {
    let mut found = log_folders(paths, near);
    if paths.host.flavor == Flavor::Posix {
        for prefix in paths.wine_prefixes() {
            let Ok(entries) = std::fs::read_dir(Path::new(&prefix).join("drive_c").join("users")) else { continue };
            let mut users: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).collect();
            users.sort();
            for user in users {
                let logs =
                    LOG_FILE[..2].iter().fold(user.join("AppData").join("Roaming"), |path, part| path.join(part));
                if logs.is_dir() && !found.contains(&logs) {
                    found.push(logs);
                }
            }
        }
    }
    found
}

/// The channel folders of a log file, read again only when the file changed.
fn read_named(file: &Path) -> Option<Vec<Named>> {
    type Kept = Vec<(PathBuf, u64, Option<SystemTime>, Vec<Named>)>;
    static KEPT: Mutex<Kept> = Mutex::new(Vec::new());
    let meta = std::fs::metadata(file).ok()?;
    let (len, modified) = (meta.len(), meta.modified().ok());
    let mut kept = KEPT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((.., found)) = kept.iter().find(|(path, l, m, _)| path == file && *l == len && *m == modified) {
        return Some(found.clone());
    }
    let bytes = std::fs::read(file).ok()?;
    let found = named_folders(&String::from_utf8_lossy(&bytes));
    kept.retain(|(path, ..)| path != file);
    kept.push((file.to_path_buf(), len, modified, found.clone()));
    Some(found)
}

/// The folder of each channel the launcher named last, as a folder of this machine and installed there
/// (`Data.p4k` or `StarCitizen.exe`), as (channel in capitals, folder), in the order of the channels. The newest line
/// of a channel counts, whichever log it stands in (the moved one and the current one are both read); if that folder
/// is not there (uninstalled, a drive the Wine prefix does not know), the channel has none: an older line does not
/// stand in for it.
///
/// `near` is a folder of the game when one is known: under Linux it tells the Wine prefix to look in first.
pub fn launched_folders(paths: &Paths, near: Option<&Path>) -> Vec<(String, String)> {
    let mut lines: Vec<(Named, Option<PathBuf>)> = Vec::new();
    for logs in log_places(paths, near) {
        let prefix = logs
            .ancestors()
            .find(|folder| folder.file_name().is_some_and(|name| name.eq_ignore_ascii_case("drive_c")))
            .and_then(Path::parent)
            .map(Path::to_path_buf);
        for name in [OLD_LOG_FILE, LOG_FILE[2]] {
            if let Some(found) = read_named(&logs.join(name)) {
                lines.extend(found.into_iter().map(|line| (line, prefix.clone())));
            }
        }
    }
    // Oldest first (a stable sort: lines of one time keep the order of the logs).
    lines.sort_by(|a, b| a.0.when.cmp(&b.0.when));
    let mut last: Vec<(String, String, Option<PathBuf>)> = Vec::new();
    for (line, prefix) in lines {
        let channel = line.channel.to_uppercase();
        last.retain(|(known, ..)| *known != channel);
        last.push((channel, line.folder, prefix));
    }
    let mut found: Vec<(String, String)> = last
        .into_iter()
        .filter_map(|(channel, folder, prefix)| {
            let here = match (paths.host.flavor, prefix) {
                (Flavor::Windows, _) => folder,
                (Flavor::Posix, Some(prefix)) => wine_folder(&prefix, &folder)?,
                (Flavor::Posix, None) => return None,
            };
            paths.is_installed_channel(&here).then_some((channel, here))
        })
        .collect();
    found.sort_by_key(|(channel, _)| CHANNELS.iter().position(|known| known == channel).unwrap_or(CHANNELS.len()));
    found
}

/// The folder the launcher starts for the channel of `configured` when it is another one: (channel, folder).
///
/// The channel is the one the folder is named after (`LIVE` for a folder that is named like none); the folder above
/// the channels leads to the channel it holds, as everywhere. Nothing for no folder set (the program takes the
/// launcher's by itself then), nothing for a channel the launcher does not name or whose folder is not installed, and
/// nothing while a game folder is given from outside (a test copy).
pub fn offer(paths: &Paths, configured: &str) -> Option<(String, String)> {
    let configured = configured.trim();
    if configured.is_empty() || paths.given().game.is_some() {
        return None;
    }
    let flavor = paths.host.flavor;
    let configured = paths.host.expanduser(configured);
    let current = paths.resolve_game_folder_near(&configured).unwrap_or(configured);
    let name = flavor.basename(current.trim_end_matches(flavor.sep())).to_uppercase();
    let channel = if CHANNELS.contains(&name.as_str()) { name } else { CHANNELS[0].to_string() };
    let (_, folder) =
        launched_folders(paths, Some(Path::new(&current))).into_iter().find(|(known, _)| *known == channel)?;
    (folder_key(&folder) != folder_key(&current)).then_some((channel, folder))
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
        assert!(!open_update(&events, Path::new(r"E:\Testumgebung\Kopie LIVE\Data.p4k")));
        assert!(!open_update(&events, Path::new("Data.p4k")));
        // The case of the folder does not matter, nor the kind of separator.
        assert!(open_update(&events, Path::new("c:/program files/roberts space industries/starcitizen/live/Data.p4k")));
    }

    /// The launcher names the folder it patches (LIVE); a channel folder that is a link to it (HOTFIX as a junction)
    /// leads to the same archive: it is patched at the same time.
    #[test]
    fn a_channel_folder_that_links_to_the_patched_one_is_patched_too() {
        let dir = crate::testdir::Dir::new("launcher-verknuepft");
        let install = dir.path().join("StarCitizen");
        let (live, hotfix) = (install.join("LIVE"), install.join("HOTFIX"));
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(live.join("Data.p4k"), b"p4k").unwrap();
        crate::testdir::link_folder(&hotfix, &live);
        let update = |folder: &Path| Update {
            channel: "LIVE".into(),
            build: "4.10.3-live.13165946".into(),
            folder: folder.to_string_lossy().into_owned(),
        };
        let open = [Event::Started(update(&install))];
        assert!(open_update(&open, &live.join("Data.p4k")), "the folder the launcher names");
        assert!(open_update(&open, &hotfix.join("Data.p4k")), "the link to it");
        let ended = [Event::Started(update(&install)), Event::Completed(update(&live))];
        assert!(!open_update(&ended, &live.join("Data.p4k")));
        assert!(!open_update(&ended, &hotfix.join("Data.p4k")), "ended for the link as well");
        // Another channel next to it is not touched.
        let ptu = install.join("PTU");
        std::fs::create_dir_all(&ptu).unwrap();
        assert!(!open_update(&open, &ptu.join("Data.p4k")));
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
        let mut vars = vec![("HOME", text.as_str()), ("VERSEKIT_HOME", text.as_str())];
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
        assert_eq!(log_folders(&paths, Some(&archive)), std::slice::from_ref(&logs));
        assert_eq!(log_file(&paths, None), None, "without the archive there is no prefix to look in");
        assert_eq!(installed_build(&paths, &archive, "live", "13165946").as_deref(), Some("4.10.3-live.13165946"));
        assert!(patching(&paths, &archive), "the patch runs in the prefix");
        let _ = std::fs::remove_dir_all(home);
    }

    /// A copy of the data names an `APPDATA` of its own, where no launcher writes: the log of the machine still names
    /// the build and still tells a patch that runs, for the reading of the name and for the lock alike.
    #[test]
    fn the_log_of_the_machine_counts_for_a_copy_of_the_data_too() {
        let (copy, paths) = machine("kopie", true);
        let (roaming, _) = machine("rechner", false);
        let logs = roaming.join("rsilauncher").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("log.log"), PATCH_START).unwrap();
        let archive = Path::new(LIVE);
        let machine_appdata = roaming.to_string_lossy().into_owned();
        let folders = log_folders_with(&paths, Some(archive), Some(&machine_appdata));
        assert_eq!(folders, std::slice::from_ref(&logs), "the folder of the copy holds no log");
        assert_eq!(installed_build_in(&folders, "live", "13165946").as_deref(), Some("4.10.3-live.13165946"));
        assert!(patching_in(&folders, archive), "the patch of the machine locks the archive");
        let own = log_folders_with(&paths, Some(archive), None);
        assert!(own.is_empty() && !patching_in(&own, archive), "the copy alone knows of no patch");
        let _ = std::fs::remove_dir_all(copy);
        let _ = std::fs::remove_dir_all(roaming);
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
            &[("HOME", text.as_str()), ("VERSEKIT_HOME", text.as_str()), ("APPDATA", text.as_str())],
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

    /// The two lines of the launcher that name a channel folder, as they stand in its log (invented folders).
    const INSTALLING: &str = r#"{ "t":"2026-11-12 19:02:11.100", "[main][info] ": "[Pipeline] Installing Star Citizen LIVE 4.10.2-live.12913470 at D:\\Spiele (x86)\\StarCitizen (type: install)"  },"#;
    const LAUNCHING: &str = r#"{ "t":"2026-11-13 20:15:42.870", "[main][info] ": "[Launcher::launch] Launching Star Citizen PTU from (E:\\Test Games\\StarCitizen\\PTU)"  },"#;

    fn named(when: &str, channel: &str, folder: &str) -> Named {
        Named { when: when.into(), channel: channel.into(), folder: folder.into() }
    }

    #[test]
    fn the_channel_folders_are_read_from_the_install_and_the_launch_line() {
        let found = named_folders(&format!("{INSTALLING}\n{PATCH_START}\n{LAUNCHING}"));
        assert_eq!(
            found,
            vec![
                // The install line names the folder above the channels: the channel folder lies in it.
                named("2026-11-12 19:02:11.100", "LIVE", r"D:\Spiele (x86)\StarCitizen\LIVE"),
                named("2026-11-13 20:15:42.870", "PTU", r"E:\Test Games\StarCitizen\PTU"),
            ]
        );
        // An update line is the same as an install line, and a line without a time is read as well.
        let update = r"[Pipeline] Installing Star Citizen EPTU 4.11.0-eptu.1 at C:\\Games\\StarCitizen (type: update)";
        assert_eq!(named_folders(update), vec![named("", "EPTU", r"C:\Games\StarCitizen\EPTU")]);
        assert!(named_folders("").is_empty());
        assert!(named_folders("[Launcher::launch] Launching Star Citizen LIVE").is_empty(), "no folder");
    }

    /// A path of the log lies in `drive_c` of the prefix for `C:`, behind the link of the prefix for another drive.
    #[test]
    fn a_windows_folder_of_the_log_is_a_folder_of_the_prefix() {
        let prefix = Path::new("/home/spieler/Games/star-citizen");
        let found = |folder: &str| wine_folder(prefix, folder);
        let base = "/home/spieler/Games/star-citizen";
        assert_eq!(
            found(r"C:\Program Files\Roberts Space Industries\StarCitizen\LIVE").as_deref(),
            Some(format!("{base}/drive_c/Program Files/Roberts Space Industries/StarCitizen/LIVE").as_str())
        );
        assert_eq!(
            found(r"c:\Games\SC\PTU\").as_deref(),
            Some(format!("{base}/drive_c/Games/SC/PTU").as_str()),
            "the case of the letter and a backslash at the end do not matter"
        );
        assert_eq!(
            found(r"D:\Spiele\StarCitizen\PTU").as_deref(),
            Some(format!("{base}/dosdevices/d:/Spiele/StarCitizen/PTU").as_str())
        );
        assert_eq!(found("/already/a/path"), None, "not a path of a drive");
        assert_eq!(found(r"C:\..\..\etc"), None, "no way out of the drive");
    }

    /// The launcher runs in the Wine prefix of the game: its log is found without the folder of the game being
    /// known, from the usual prefixes.
    #[test]
    fn under_wine_the_folders_come_from_the_log_of_the_prefix() {
        let home = std::env::temp_dir().join(format!("versekit-launcher-ordner-wine-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&home);
        let text = home.to_string_lossy().replace('\\', "/");
        let paths = Paths::new(crate::paths::Host::new(
            crate::pypath::Flavor::Posix,
            &[("HOME", text.as_str()), ("VERSEKIT_HOME", text.as_str())],
            &text,
        ));
        let drive_c = home.join("Games").join("star-citizen").join("drive_c");
        let channel =
            |name: &str| drive_c.join("Program Files").join("Roberts Space Industries").join("StarCitizen").join(name);
        let (live, ptu) = (channel("LIVE"), channel("PTU"));
        for folder in [&live, &ptu] {
            std::fs::create_dir_all(folder.join("Bin64")).unwrap();
        }
        std::fs::write(live.join("Data.p4k"), "x").unwrap();
        std::fs::write(ptu.join("Bin64").join("StarCitizen.exe"), "x").unwrap();
        let logs =
            drive_c.join("users").join("spieler").join("AppData").join("Roaming").join("rsilauncher").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let text = r#"{ "t":"2026-11-12 19:02:11.100", "[main][info] ": "[Launcher::launch] Launching Star Citizen LIVE from (C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE)"  },
{ "t":"2026-11-12 19:05:00.000", "[main][info] ": "[Pipeline] Installing Star Citizen PTU 4.11.0-ptu.1 at C:\\Program Files\\Roberts Space Industries\\StarCitizen (type: install)"  },
{ "t":"2026-11-12 19:06:00.000", "[main][info] ": "[Launcher::launch] Launching Star Citizen EPTU from (C:\\Program Files\\Roberts Space Industries\\StarCitizen\\EPTU)"  },"#;
        std::fs::write(logs.join("log.log"), text).unwrap();
        let found: Vec<(String, String)> = launched_folders(&paths, None)
            .into_iter()
            .map(|(channel, folder)| (channel, folder.replace('\\', "/")))
            .collect();
        let expect = |folder: &Path| folder.to_string_lossy().replace('\\', "/");
        assert_eq!(
            found,
            [("LIVE".to_string(), expect(&live)), ("PTU".to_string(), expect(&ptu))],
            "EPTU is named but not installed: left out"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// Reads the launcher log of the computer it runs on (reads only, nothing is written) and says in numbers and
    /// yes or no whether the lines were understood; it prints no folder. By hand:
    /// `cargo test -p versekit-core --lib the_real_log -- --ignored --nocapture`.
    #[test]
    #[ignore = "reads the launcher log of this computer"]
    fn the_real_log_of_this_computer_names_installed_folders() {
        // Not `Host::current()`: the tests run on a machine of their own, which has no launcher log.
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        let paths = Paths::new(crate::paths::Host::new(
            crate::pypath::Flavor::native(),
            &[("APPDATA", appdata.as_str()), ("HOME", appdata.as_str())],
            &appdata,
        ));
        let mut lines = 0;
        for folder in log_places(&paths, None) {
            for name in [OLD_LOG_FILE, LOG_FILE[2]] {
                lines += read_named(&folder.join(name)).map_or(0, |found| found.len());
            }
        }
        let launched = launched_folders(&paths, None);
        println!("lines naming a folder: {lines}");
        for (channel, folder) in &launched {
            println!(
                "{channel}: installed={}, game log={}",
                paths.is_installed_channel(folder),
                paths.game_log(Some(folder)).is_some()
            );
        }
        println!("channels found: {}, game folder found at all: {}", launched.len(), paths.game_folder().is_some());
        if let Some(game) = paths.game_folder() {
            println!("offer for the game folder found: {}", offer(&paths, &game).is_some());
        }
        assert!(lines > 0 && !launched.is_empty(), "the launcher of this computer names a folder it installed");
    }

    /// The newest line of a channel wins, whichever log it is in.
    #[test]
    fn the_newest_line_of_a_channel_wins() {
        let (home, paths) = machine("neueste", true);
        let games = home.join("Spiele");
        let make = |name: &str| {
            let live = games.join(name).join("StarCitizen").join("LIVE");
            std::fs::create_dir_all(&live).unwrap();
            std::fs::write(live.join("Data.p4k"), "x").unwrap();
            live.to_string_lossy().replace('\\', "\\\\")
        };
        let (old, new) = (make("alt"), make("neu"));
        let line = |when: &str, folder: &str| {
            format!(
                r#"{{ "t":"{when}", "[main][info] ": "[Launcher::launch] Launching Star Citizen LIVE from ({folder})"  }},"#
            )
        };
        let logs = home.join("rsilauncher").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join(OLD_LOG_FILE), line("2026-11-01 10:00:00.000", &old)).unwrap();
        std::fs::write(logs.join("log.log"), line("2026-11-13 10:00:00.000", &new)).unwrap();
        let expected = games.join("neu").join("StarCitizen").join("LIVE").to_string_lossy().into_owned();
        assert_eq!(launched_folders(&paths, None), [("LIVE".to_string(), expected)]);
        // A line of a later time in the moved log counts by its time, not by the file it stands in.
        std::fs::write(logs.join(OLD_LOG_FILE), line("2026-12-01 10:00:00.000", &old)).unwrap();
        let expected = games.join("alt").join("StarCitizen").join("LIVE").to_string_lossy().into_owned();
        assert_eq!(launched_folders(&paths, None), [("LIVE".to_string(), expected)]);
        let _ = std::fs::remove_dir_all(home);
    }

    /// The offer: only for another folder of the same channel, and only when the launcher's one is installed.
    #[test]
    fn another_folder_is_offered_only_for_the_channel_of_the_one_set() {
        let (home, paths) = machine("angebot", true);
        let real = home.join("Echt").join("StarCitizen").join("LIVE");
        let old = home.join("Alt").join("StarCitizen").join("LIVE");
        for folder in [&real, &old] {
            std::fs::create_dir_all(folder).unwrap();
            std::fs::write(folder.join("Data.p4k"), "x").unwrap();
        }
        let logs = home.join("rsilauncher").join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        let line = format!(
            r#"{{ "t":"2026-11-13 10:00:00.000", "[main][info] ": "[Launcher::launch] Launching Star Citizen LIVE from ({})"  }},"#,
            real.to_string_lossy().replace('\\', "\\\\")
        );
        std::fs::write(logs.join("log.log"), line).unwrap();
        let real_text = real.to_string_lossy().into_owned();
        let expected = Some(("LIVE".to_string(), real_text.clone()));
        assert_eq!(offer(&paths, &old.to_string_lossy()), expected, "the folder set is another one");
        assert_eq!(offer(&paths, &format!("{}/", old.to_string_lossy())), expected, "a separator at the end");
        assert_eq!(offer(&paths, &real_text), None, "the same folder: nothing to say");
        if cfg!(windows) {
            assert_eq!(offer(&paths, &real_text.to_uppercase()), None, "the case of a Windows folder does not matter");
        }
        assert_eq!(offer(&paths, ""), None, "nothing set: the folder is taken by itself, not offered");
        assert_eq!(offer(&paths, &home.join("StarCitizen").join("PTU").to_string_lossy()), None, "another channel");
        // The folder above the channels leads to the channel the launcher starts.
        assert_eq!(offer(&paths, &real.parent().unwrap().to_string_lossy()), None);
        // Gone (renamed to HOTFIX, uninstalled): the launcher's folder is no offer.
        std::fs::remove_file(real.join("Data.p4k")).unwrap();
        assert_eq!(offer(&paths, &old.to_string_lossy()), None);
        let _ = std::fs::remove_dir_all(home);
    }
}
