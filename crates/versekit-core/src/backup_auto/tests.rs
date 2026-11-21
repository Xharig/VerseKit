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

//! Only throwaway folders: a Linux-style home with a data folder and a LIVE folder under it.

use super::*;
use crate::paths::{ENV_INSTALL_DIR, Host};
use crate::pypath::Flavor;

struct World {
    home: String,
    paths: Paths,
    live: String,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn world(name: &str) -> World {
    let home = crate::testdir::fresh(&format!("backupauto-{name}")).to_string_lossy().replace('\\', "/");
    let data = format!("{home}/daten");
    let live = format!("{home}/StarCitizen/LIVE");
    for folder in [&data, &live] {
        std::fs::create_dir_all(folder).expect("folder");
    }
    std::fs::write(format!("{data}/bestand.json"), "{}").expect("own data");
    // A log of two hours ago: the game folder is known and the game is quiet.
    std::fs::write(format!("{live}/Game.log"), "x").expect("log");
    let at = std::time::SystemTime::now() - std::time::Duration::from_secs(7200);
    std::fs::File::options()
        .write(true)
        .open(format!("{live}/Game.log"))
        .and_then(|f| f.set_modified(at))
        .expect("age");
    let vars = [
        ("VERSEKIT_HOME", data.as_str()),
        ("HOME", home.as_str()),
        (game_start::ENV_NO_PROCESS, "1"),
        (ENV_INSTALL_DIR, live.as_str()),
    ];
    let paths = Paths::new(Host::new(Flavor::Posix, &vars, &home));
    assert_eq!(paths.game_folder().as_deref(), Some(live.as_str()));
    World { home, paths, live }
}

fn touch(folder: &str, name: &str) {
    std::fs::create_dir_all(folder).expect("folder");
    std::fs::write(Path::new(folder).join(name), "x").expect("file");
}

/// Only the newest stay; the newest stays even where nothing should; a file of another form is never touched.
#[test]
fn the_oldest_go_the_newest_always_stays_and_nothing_foreign_is_removed() {
    let w = world("aufraeumen");
    let folder = folder(&w.paths);
    let own = [
        "auto-2026-11-01-080000-start-5.4.0.zip",
        "auto-2026-11-02-080000-spielende-5.4.0.zip",
        "auto-2026-11-03-080000-start-5.4.0.zip",
        "auto-2026-11-04-080000-start-5.4.0.zip",
    ];
    for name in own {
        touch(&folder, name);
    }
    let foreign = ["meine-sicherung.zip", "auto-2026-10-01-080000-start-5.4.0.zip.txt", "auto-alt.zip"];
    for name in foreign {
        touch(&folder, name);
    }
    assert_eq!(prune(&folder, 2), [own[0], own[1]]);
    assert_eq!(list(&folder), [own[2], own[3]]);
    assert_eq!(prune(&folder, 0), [own[2]], "keep 0 is keep 1");
    assert_eq!(list(&folder), [own[3]], "the newest stays");
    assert!(prune(&folder, 0).is_empty());
    for name in foreign {
        assert!(Path::new(&folder).join(name).is_file(), "{name} stays");
    }
}

/// A backup written by itself holds the data and the files of the game, is a backup of this program, is cut to the
/// number set, and is not written where it is switched off.
#[test]
fn a_backup_written_by_itself_holds_the_game_files_and_keeps_the_number_set() {
    let w = world("schreiben");
    std::fs::create_dir_all(format!("{}/USER/Client/0/Profiles/default", w.live)).expect("folder");
    std::fs::write(format!("{}/USER/Client/0/Profiles/default/actionmaps.xml", w.live), "<ActionMaps/>").expect("binding");
    std::fs::write(format!("{}/user.cfg", w.live), "sys_maxfps = 90\n").expect("cfg");
    let folder = folder(&w.paths);
    touch(&folder, "auto-2020-01-01-000000-start-5.0.0.zip");
    touch(&folder, "auto-2020-01-02-000000-start-5.0.0.zip");
    touch(&folder, "auto-2020-01-03-000000-start-5.0.0.zip");
    assert!(set_keep(&w.paths, 2));
    assert_eq!(keep(&w.paths), 2);
    assert_eq!(list(&folder).len(), 3, "setting the number removes nothing by itself");

    let written = write(&w.paths, "5.4.0", Trigger::GameEnd).expect("written").expect("switched on");
    let name = Path::new(&written).file_name().expect("name").to_string_lossy().into_owned();
    assert!(is_own(&name) && name.contains("-spielende-5.4.0"), "{name}");
    assert!(backup::check(&written).is_some(), "a backup of this program");
    let data = std::fs::read(&written).expect("archive");
    let names: Vec<String> = crate::zip::entries(&data).expect("zip").into_iter().map(|e| e.name).collect();
    assert!(names.contains(&"bestand.json".to_string()), "{names:?}");
    assert!(names.contains(&format!("{}LIVE/user.cfg", backup_game::CHANNELS_PREFIX)), "{names:?}");
    assert!(names.contains(&format!("{}LIVE/actionmaps.xml", backup_game::CHANNELS_PREFIX)), "{names:?}");
    assert_eq!(list(&folder), ["auto-2020-01-03-000000-start-5.0.0.zip".to_string(), name], "cut to two");
    // It is no backup the player made: the time of the last backup stays.
    assert!(backup::last_written(&w.paths).is_empty());

    assert!(set_enabled(&w.paths, Trigger::GameEnd, false));
    assert_eq!(write(&w.paths, "5.4.0", Trigger::GameEnd), Ok(None));
    assert!(enabled(&w.paths, Trigger::Start), "the other one stays on");
}

/// The number set is held within its limits.
#[test]
fn the_number_to_keep_is_held_within_its_limits() {
    let w = world("grenzen");
    assert_eq!(keep(&w.paths), usize::try_from(KEEP_DEFAULT).expect("number"));
    set_keep(&w.paths, 0);
    assert_eq!(keep(&w.paths), 1);
    set_keep(&w.paths, 1000);
    assert_eq!(keep(&w.paths), usize::try_from(KEEP_MAX).expect("number"));
}

/// The start makes one backup a day: due while there is none of today, not after it, not when switched off.
#[test]
fn the_start_makes_one_backup_a_day() {
    let w = world("start");
    assert!(due_at_start(&w.paths));
    // One of an earlier day does not count.
    touch(&folder(&w.paths), "auto-2020-01-01-000000-start-5.0.0.zip");
    assert!(due_at_start(&w.paths));
    write(&w.paths, "5.4.0", Trigger::Start).expect("written");
    assert!(!due_at_start(&w.paths), "today has one");
    let w = world("start-aus");
    set_enabled(&w.paths, Trigger::Start, false);
    assert!(!due_at_start(&w.paths));
}

/// The end of the game is seen once, after it was seen running; a state nobody can tell changes nothing.
#[test]
fn the_end_of_the_game_is_seen_once_after_it_ran() {
    let mut watch = GameWatch::default();
    assert!(!watch.step(RunState::Idle), "quiet from the start: no end");
    assert!(!watch.step(RunState::Running));
    assert!(!watch.step(RunState::Unknown), "unknown is no end");
    assert!(!watch.step(RunState::Running));
    assert!(watch.step(RunState::Idle), "the end");
    assert!(!watch.step(RunState::Idle), "only once");
}

/// Removing the own data takes the backups written by themselves along, by the form of their names.
#[test]
fn the_removal_of_the_own_data_knows_the_folder_and_its_names() {
    let (folder, pattern) = crate::data_removal::INNER_FOLDERS
        .iter()
        .find(|(name, _)| *name == FOLDER)
        .expect("the folder is known");
    assert_eq!(*folder, FOLDER);
    let pattern = Regex::new(pattern).expect("pattern");
    assert!(pattern.is_match("auto-2026-11-19-081500-start-5.4.0.zip"));
    assert!(pattern.is_match("auto-2026-11-19-081500-spielende-5.4.0.zip.teil"));
    assert!(!pattern.is_match("meine-sicherung.zip"));
}
