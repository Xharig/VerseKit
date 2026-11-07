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

//! The dialog of the desktop under Linux through its helper programs: `kdialog` (KDE Plasma) first, then
//! `zenity` (GNOME, found on most systems). The first one on the path that runs gives the answer; when none
//! gets through, the caller shows its own dialog (the XDG portal).
//!
//! ⚠ Exit code 1 is "cancelled", a valid answer that ends the matter. Any other end (another code, exit 0
//! without a path, no start, no end within [`PATIENCE`]) is a failed helper: recorded in the error log, and the
//! next one is tried; a helper that dies at once (a library of an AppImage it cannot load) would otherwise look
//! like a button that does nothing. Helpers run with the environment of [`versekit_core::updater::clean_environment`].
//!
//! The command lines and the judging of an end are built everywhere, so that their tests run on every system;
//! only Linux runs them ([`HERE`]).

#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{Answer, Filters};

/// Whether this system asks the helpers first: Linux and the other Unix desktops, not macOS, whose own dialog
/// comes through the caller's.
pub(super) const HERE: bool = cfg!(all(unix, not(target_os = "macos")));

/// How long a dialog may stay open before it is given up: a person sits before it and searches.
pub(super) const PATIENCE: Duration = Duration::from_secs(600);

/// What is asked for.
#[derive(Clone, Copy, Debug)]
pub(super) enum Ask<'a> {
    /// An existing file of one of the types.
    Open(Filters<'a>),
    /// Where to save, with the suggested file name.
    Save(Filters<'a>, &'a str),
    Folder,
}

impl Ask<'_> {
    /// The label of the error log for this kind of dialog.
    pub(super) fn origin(&self) -> &'static str {
        match self {
            Ask::Open(_) => "file_picker.open",
            Ask::Save(..) => "file_picker.save",
            Ask::Folder => "file_picker.folder",
        }
    }
}

/// The pattern of one file type: `*.json`, several separated by a space; the ending `*` is every file.
fn patterns(endings: &[&str]) -> String {
    endings.iter().map(|e| if *e == "*" { "*".to_string() } else { format!("*.{e}") }).collect::<Vec<_>>().join(" ")
}

/// The filter of kdialog: every pattern, a bar, every name (`*.json *.csv|JSON CSV`).
fn kdialog_filter(filters: Filters) -> String {
    let all: Vec<String> = filters.iter().map(|(_, endings)| patterns(endings)).collect();
    let names: Vec<&str> = filters.iter().map(|(name, _)| *name).collect();
    format!("{}|{}", all.join(" "), names.join(" "))
}

/// `base` and `name` joined as a POSIX path: an absolute `name` stands for itself.
fn join(base: &str, name: &str) -> String {
    if name.starts_with('/') || base.is_empty() {
        name.to_string()
    } else if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

/// `start` as zenity takes a folder: with one slash at the end.
fn as_folder(start: &str) -> String {
    format!("{}/", start.trim_end_matches('/'))
}

/// The command lines to try, in order: kdialog, then zenity. `home` stands in for a missing `start`; an empty
/// `start` counts as missing. Without filters kdialog gets none and zenity shows every file.
pub(super) fn commands(ask: Ask, title: &str, start: Option<&str>, home: &str) -> Vec<Vec<String>> {
    let start = start.filter(|s| !s.is_empty());
    let begin = start.unwrap_or(home);
    let owned = |parts: &[&str]| parts.iter().map(|p| (*p).to_string()).collect::<Vec<String>>();
    match ask {
        Ask::Folder => {
            let kdialog = owned(&["kdialog", "--getexistingdirectory", begin, "--title", title]);
            let mut zenity = owned(&["zenity", "--file-selection", "--directory", "--title", title]);
            if let Some(start) = start {
                zenity.extend(["--filename".to_string(), as_folder(start)]);
            }
            vec![kdialog, zenity]
        }
        Ask::Open(filters) => {
            let mut kdialog = owned(&["kdialog", "--getopenfilename", begin]);
            if !filters.is_empty() {
                kdialog.push(kdialog_filter(filters));
            }
            kdialog.extend(owned(&["--title", title]));
            let mut zenity = owned(&["zenity", "--file-selection", "--title", title]);
            zenity
                .extend(filters.iter().map(|(name, endings)| format!("--file-filter={name} | {}", patterns(endings))));
            if let Some(start) = start {
                zenity.extend(["--filename".to_string(), as_folder(start)]);
            }
            vec![kdialog, zenity]
        }
        Ask::Save(filters, name) => {
            let place = if name.is_empty() { begin.to_string() } else { join(begin, name) };
            let mut kdialog = owned(&["kdialog", "--getsavefilename", &place]);
            if !filters.is_empty() {
                kdialog.push(kdialog_filter(filters));
            }
            kdialog.extend(owned(&["--title", title]));
            let zenity =
                owned(&["zenity", "--file-selection", "--save", "--confirm-overwrite", "--title", title, "--filename"]);
            let zenity = zenity.into_iter().chain([place]).collect();
            vec![kdialog, zenity]
        }
    }
}

/// How one helper ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Ended {
    Chosen(String),
    Cancelled,
    /// The helper failed; the text for the error log.
    Failed(String),
}

/// The end of a helper from its exit code (negative: the signal that ended it) and its output: a path on
/// exit 0, cancelled on exit 1 (whatever it printed), failed otherwise, with the first 200 characters of its
/// error output.
pub(super) fn judge(code: i32, stdout: &str, stderr: &str) -> Ended {
    let chosen = versekit_core::text::py_strip(stdout);
    if code == 0 && !chosen.is_empty() {
        return Ended::Chosen(chosen.to_string());
    }
    if code == 1 {
        return Ended::Cancelled;
    }
    let said: String = stderr.chars().take(200).collect();
    Ended::Failed(format!("Code {code}: {said}"))
}

/// Asks the helpers of `commands` in order: the answer of the first that does not fail, `None` when every one
/// is missing or failed. `on_path` tells whether a program is there, `run` runs a command line and gives its exit
/// code and output (or why it could not), `record` takes each failure (label, text) for the error log.
pub(super) fn ask_helpers(
    commands: &[Vec<String>],
    origin: &str,
    on_path: impl Fn(&str) -> bool,
    mut run: impl FnMut(&[String]) -> Result<(i32, String, String), String>,
    mut record: impl FnMut(&str, &str),
) -> Option<Answer> {
    for command in commands {
        let Some(program) = command.first().filter(|p| on_path(p)) else { continue };
        let label = format!("{origin}:{program}");
        let ended = match run(command) {
            Ok((code, stdout, stderr)) => judge(code, &stdout, &stderr),
            Err(why) => Ended::Failed(why),
        };
        match ended {
            Ended::Chosen(path) => return Some(Answer::Chosen(path)),
            Ended::Cancelled => return Some(Answer::Cancelled),
            Ended::Failed(text) => record(&label, &text),
        }
    }
    None
}

/// Whether `name` is a program in one of the folders of `path` (the variable `PATH`), as the shell finds it.
pub(super) fn on_path(name: &str, path: Option<&std::ffi::OsStr>) -> bool {
    path.is_some_and(|path| std::env::split_paths(path).any(|folder| runnable(&folder.join(name))))
}

fn runnable(file: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(file) else { return false };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// Runs `command` with the environment `env` (in place of this one) and waits up to `patience` for its end:
/// the exit code (negative: the signal that ended it) and what it printed. A helper still open then is ended.
pub(super) fn run(
    command: &[String],
    env: &[(String, String)],
    patience: Duration,
) -> Result<(i32, String, String), String> {
    let (program, args) = command.split_first().ok_or("leere Befehlszeile")?;
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .envs(env.iter().cloned())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let read = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        })
    };
    let out = read(child.stdout.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>));
    let err = read(child.stderr.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>));
    let began = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if began.elapsed() < patience => std::thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("nach {} s nicht beendet", patience.as_secs()));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let (stdout, stderr) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    Ok((exit_code(status), stdout, stderr))
}

fn exit_code(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.code().unwrap_or_else(|| -status.signal().unwrap_or(0))
    }
    #[cfg(not(unix))]
    {
        status.code().unwrap_or(-1)
    }
}

/// The dialog of the desktop for `ask`, where this system has helpers ([`HERE`]): `None` when none is there or
/// every one failed (each failure recorded), and always elsewhere.
pub(super) fn ask_desktop(commands: &[Vec<String>], origin: &str) -> Option<Answer> {
    if commands.is_empty() {
        return None;
    }
    let env = versekit_core::updater::clean_environment(std::env::vars());
    let path = std::env::var_os("PATH");
    ask_helpers(
        commands,
        origin,
        |program| on_path(program, path.as_deref()),
        |command| run(command, &env, PATIENCE),
        |label, text| versekit_core::errors::record(label, &text),
    )
}

/// The command lines for `ask` on this system: none where it asks no helper.
pub(super) fn for_here(ask: Ask, title: &str, start: Option<&str>) -> Vec<Vec<String>> {
    if !HERE {
        return Vec::new();
    }
    let home = std::env::var("HOME").ok().filter(|h| !h.is_empty()).unwrap_or_else(|| "~".to_string());
    commands(ask, title, start, &home)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| (*p).to_string()).collect()
    }

    #[test]
    fn the_folder_is_asked_of_kdialog_then_of_zenity_from_the_start_or_home() {
        let with = commands(Ask::Folder, "Ordner", Some("/spiel/LIVE/"), "/home/x");
        assert_eq!(
            with,
            vec![
                line(&["kdialog", "--getexistingdirectory", "/spiel/LIVE/", "--title", "Ordner"]),
                line(&["zenity", "--file-selection", "--directory", "--title", "Ordner", "--filename", "/spiel/LIVE/"]),
            ]
        );
        let without = commands(Ask::Folder, "Ordner", None, "/home/x");
        assert_eq!(without[0], line(&["kdialog", "--getexistingdirectory", "/home/x", "--title", "Ordner"]));
        assert_eq!(without[1], line(&["zenity", "--file-selection", "--directory", "--title", "Ordner"]));
        assert_eq!(commands(Ask::Folder, "Ordner", Some(""), "/home/x"), without, "an empty start is no start");
    }

    #[test]
    fn a_file_to_open_carries_the_filters_in_the_writing_of_each_helper() {
        let filters: Filters = &[("JSON", &["json"]), ("Bilder", &["png", "jpg"]), ("Alle", &["*"])];
        let asked = commands(Ask::Open(filters), "Einlesen", Some("/daten"), "/home/x");
        assert_eq!(
            asked[0],
            line(&[
                "kdialog",
                "--getopenfilename",
                "/daten",
                "*.json *.png *.jpg *|JSON Bilder Alle",
                "--title",
                "Einlesen"
            ])
        );
        assert_eq!(
            asked[1],
            line(&[
                "zenity",
                "--file-selection",
                "--title",
                "Einlesen",
                "--file-filter=JSON | *.json",
                "--file-filter=Bilder | *.png *.jpg",
                "--file-filter=Alle | *",
                "--filename",
                "/daten/",
            ])
        );
        let any = commands(Ask::Open(&[]), "Datei", None, "/home/x");
        assert_eq!(any[0], line(&["kdialog", "--getopenfilename", "/home/x", "--title", "Datei"]));
        assert_eq!(any[1], line(&["zenity", "--file-selection", "--title", "Datei"]));
    }

    #[test]
    fn a_place_to_save_is_the_suggested_name_in_the_start_folder() {
        let filters: Filters = &[("ZIP", &["zip"])];
        let asked = commands(Ask::Save(filters, "sicherung.zip"), "Sichern", None, "/home/x");
        assert_eq!(
            asked,
            vec![
                line(&["kdialog", "--getsavefilename", "/home/x/sicherung.zip", "*.zip|ZIP", "--title", "Sichern"]),
                line(&[
                    "zenity",
                    "--file-selection",
                    "--save",
                    "--confirm-overwrite",
                    "--title",
                    "Sichern",
                    "--filename",
                    "/home/x/sicherung.zip",
                ]),
            ]
        );
        let in_folder = commands(Ask::Save(filters, "a.zip"), "S", Some("/ablage/"), "/home/x");
        assert_eq!(in_folder[0][2], "/ablage/a.zip");
        let nameless = commands(Ask::Save(filters, ""), "S", Some("/ablage"), "/home/x");
        assert_eq!(nameless[1].last().map(String::as_str), Some("/ablage"));
        assert_eq!(commands(Ask::Save(filters, "/tmp/b.zip"), "S", None, "/home/x")[0][2], "/tmp/b.zip");
    }

    #[test]
    fn exit_one_is_cancelled_and_every_other_end_without_a_path_is_a_failure() {
        assert_eq!(judge(0, " /home/x/a.json\n", ""), Ended::Chosen("/home/x/a.json".into()));
        assert_eq!(judge(1, "", ""), Ended::Cancelled);
        assert_eq!(judge(1, "/home/x/a.json\n", ""), Ended::Cancelled, "exit 1 is cancelled, whatever it printed");
        assert_eq!(judge(0, "  \n", ""), Ended::Failed("Code 0: ".into()), "exit 0 without a path");
        assert_eq!(judge(2, "", "kaputt"), Ended::Failed("Code 2: kaputt".into()));
        assert_eq!(judge(-11, "", ""), Ended::Failed("Code -11: ".into()), "ended by a signal");
        let long = "x".repeat(300);
        assert_eq!(judge(127, "", &long), Ended::Failed(format!("Code 127: {}", "x".repeat(200))));
    }

    /// The order of the helpers, the cancel that ends the matter, the failure that goes on to the next one and
    /// is recorded, and the way back to the caller's dialog: all without starting a process.
    #[test]
    fn the_helpers_are_asked_in_order_and_only_a_failure_goes_on() {
        let commands = vec![line(&["kdialog", "--a"]), line(&["zenity", "--b"])];
        let ask = |there: &[&str], ends: Vec<Result<(i32, String, String), String>>| {
            let mut ends = ends.into_iter();
            let (mut ran, mut recorded) = (Vec::new(), Vec::new());
            let answer = ask_helpers(
                &commands,
                "file_picker.open",
                |p| there.contains(&p),
                |c| {
                    ran.push(c[0].clone());
                    ends.next().expect("an end for each run")
                },
                |label, text| recorded.push(format!("{label} {text}")),
            );
            (answer, ran, recorded)
        };
        let chosen = Ok((0, "/a.json\n".to_string(), String::new()));
        let cancelled = Ok((1, String::new(), String::new()));
        let crashed = Ok((134, String::new(), "symbol lookup error".to_string()));

        let (answer, ran, recorded) = ask(&["kdialog", "zenity"], vec![chosen.clone()]);
        assert_eq!((answer, ran, recorded.len()), (Some(Answer::Chosen("/a.json".into())), vec!["kdialog".into()], 0));

        let (answer, ran, _) = ask(&["kdialog", "zenity"], vec![cancelled.clone()]);
        assert_eq!((answer, ran), (Some(Answer::Cancelled), vec!["kdialog".to_string()]), "a cancel ends the matter");

        let (answer, ran, recorded) = ask(&["kdialog", "zenity"], vec![crashed.clone(), chosen.clone()]);
        assert_eq!(answer, Some(Answer::Chosen("/a.json".into())), "a failed helper hands over to the next");
        assert_eq!(ran, vec!["kdialog".to_string(), "zenity".to_string()]);
        assert_eq!(recorded, vec!["file_picker.open:kdialog Code 134: symbol lookup error".to_string()]);

        let (answer, ran, _) = ask(&["zenity"], vec![cancelled]);
        assert_eq!(
            (answer, ran),
            (Some(Answer::Cancelled), vec!["zenity".to_string()]),
            "a missing one is passed over"
        );

        let (answer, _, recorded) = ask(&["kdialog", "zenity"], vec![crashed, Err("No such file".into())]);
        assert_eq!(answer, None, "every helper failed: the caller's own dialog");
        assert_eq!(
            recorded,
            vec![
                "file_picker.open:kdialog Code 134: symbol lookup error".to_string(),
                "file_picker.open:zenity No such file".to_string(),
            ]
        );

        let (answer, ran, _) = ask(&[], vec![]);
        assert_eq!((answer, ran.len()), (None, 0), "no helper there: the caller's own dialog");
    }

    #[test]
    fn a_program_is_found_only_in_a_folder_of_the_path() {
        let folder = std::env::temp_dir().join(format!("versekit-helfer-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("folder");
        let program = folder.join("zenity");
        std::fs::write(&program, "#!/bin/sh\n").expect("file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("mode");
        }
        let path = std::env::join_paths([std::env::temp_dir(), folder.clone()]).expect("path");
        assert!(on_path("zenity", Some(&path)));
        assert!(!on_path("kdialog", Some(&path)));
        assert!(!on_path("zenity", None));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn no_helper_is_asked_where_the_system_has_its_own_dialog() {
        let asked = for_here(Ask::Folder, "Ordner", None);
        assert_eq!(asked.is_empty(), !HERE);
        assert_eq!(ask_desktop(&[], "file_picker.folder"), None);
    }
}
