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

//! Only throwaway folders: a Linux-style home with a LIVE and a PTU folder under it. The installation of the
//! machine is never touched: the folders are checked to be the test ones before anything is written.

use super::*;
use crate::game_start;
use crate::paths::{ENV_INSTALL_DIR, Host};
use crate::pypath::Flavor;

const ATTRIBUTES: &str = "<Attributes><Attr name=\"FOV\" value=\"80\"/></Attributes>";
const ACTIONMAPS: &str = "<ActionMaps><actionmap name=\"seat_general\"/></ActionMaps>";
const PROFILE: &str = "<ActionMaps><actionmap name=\"profile_a\"/></ActionMaps>";

struct World {
    home: String,
    paths: Paths,
    live: String,
    ptu: String,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// A user whose game is quiet (`age` seconds since the log was written; `Some(5)`: it runs).
fn world(name: &str, age: u64) -> World {
    let home = crate::testdir::fresh(&format!("channelcopy-{name}")).to_string_lossy().replace('\\', "/");
    let live = format!("{home}/StarCitizen/LIVE");
    let ptu = format!("{home}/StarCitizen/PTU");
    std::fs::create_dir_all(&ptu).expect("folder");
    std::fs::create_dir_all(&live).expect("folder");
    std::fs::write(format!("{live}/Game.log"), "x").expect("log");
    let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
    std::fs::File::options()
        .write(true)
        .open(format!("{live}/Game.log"))
        .and_then(|f| f.set_modified(at))
        .expect("age");
    let vars = [("VERSEKIT_HOME", home.as_str()), ("HOME", home.as_str()), (game_start::ENV_NO_PROCESS, "1"), (ENV_INSTALL_DIR, live.as_str())];
    let paths = Paths::new(Host::new(Flavor::Posix, &vars, &home));
    // Never the installation of the machine.
    assert_eq!(paths.game_folder().as_deref(), Some(live.as_str()));
    World { home, paths, live, ptu }
}

fn put(path: &str, text: &str) {
    std::fs::create_dir_all(Path::new(path).parent().expect("parent")).expect("folder");
    std::fs::write(path, text).expect("file");
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The source channel: all three parts, with a language line of its own.
fn fill_live(w: &World) {
    put(&format!("{}/USER/Client/0/Profiles/default/attributes.xml", w.live), ATTRIBUTES);
    put(&format!("{}/USER/Client/0/Profiles/default/actionmaps.xml", w.live), ACTIONMAPS);
    put(&format!("{}/USER/Client/0/controls/mappings/profile_a.xml", w.live), PROFILE);
    put(&format!("{}/user.cfg", w.live), "g_language = german_(germany)\nr_displayinfo = 3\nsys_maxfps = 90\n");
}

#[test]
fn all_parts_arrive_in_the_target_with_a_copy_of_what_was_replaced_and_its_own_language_line() {
    let w = world("alle", 7200);
    fill_live(&w);
    let attributes = format!("{}/USER/Client/0/Profiles/default/attributes.xml", w.ptu);
    put(&attributes, "<Attributes old=\"1\"/>");
    put(&format!("{}/user.cfg", w.ptu), "g_language = english\nsys_maxfps = 30\nonly_here = 1\n");
    // A saved profile only the target has stays.
    let kept = format!("{}/USER/Client/0/controls/mappings/mine.xml", w.ptu);
    put(&kept, PROFILE);

    let report = copy(&w.paths, &w.live, &w.ptu, &PARTS).expect("copied");
    assert_eq!(read(&attributes), ATTRIBUTES);
    assert_eq!(read(&format!("{}/USER/Client/0/Profiles/default/actionmaps.xml", w.ptu)), ACTIONMAPS);
    assert_eq!(read(&format!("{}/USER/Client/0/controls/mappings/profile_a.xml", w.ptu)), PROFILE);
    assert_eq!(read(&kept), PROFILE, "profiles only the target has stay");
    let cfg = read(&format!("{}/user.cfg", w.ptu));
    assert!(cfg.contains("sys_maxfps = 90") && cfg.contains("r_displayinfo = 3"), "{cfg}");
    assert!(!cfg.contains("only_here"), "the own lines are replaced: {cfg}");
    assert!(cfg.contains("g_language = english") && !cfg.contains("german_(germany)"), "the language stays: {cfg}");
    assert_eq!(report.written(), 4, "{report:?}");
    // The two files that were replaced have a copy that holds the old content.
    let copies = game_files::backups(&attributes);
    assert_eq!(copies.len(), 1);
    assert_eq!(read(&copies[0]), "<Attributes old=\"1\"/>");
    assert_eq!(report.backups(), 2, "{report:?}");
    // The source is untouched.
    assert!(read(&format!("{}/user.cfg", w.live)).contains("german_(germany)"));
}

#[test]
fn a_second_copy_changes_nothing_and_says_so() {
    let w = world("zweimal", 7200);
    fill_live(&w);
    copy(&w.paths, &w.live, &w.ptu, &PARTS).expect("first");
    let again = copy(&w.paths, &w.live, &w.ptu, &PARTS).expect("second");
    assert_eq!(again.written(), 0, "{again:?}");
    assert_eq!(again.unchanged(), 4, "{again:?}");
    assert_eq!(again.backups(), 0);
}

#[test]
fn nothing_is_written_while_the_game_runs() {
    let w = world("laeuft", 5);
    fill_live(&w);
    let error = copy(&w.paths, &w.live, &w.ptu, &PARTS).expect_err("refused");
    assert!(matches!(error, CopyError::GameNotQuiet(_)), "{error:?}");
    assert!(!Path::new(&format!("{}/user.cfg", w.ptu)).exists());
    assert!(!Path::new(&format!("{}/USER", w.ptu)).exists(), "not even a folder");
}

#[test]
fn a_damaged_source_stops_everything_before_anything_is_written() {
    let w = world("kaputt", 7200);
    fill_live(&w);
    put(&format!("{}/USER/Client/0/Profiles/default/attributes.xml", w.live), "not xml <<<");
    let error = copy(&w.paths, &w.live, &w.ptu, &PARTS).expect_err("refused");
    assert!(matches!(error, CopyError::Damaged(Part::Graphics, _)), "{error:?}");
    assert!(!Path::new(&format!("{}/user.cfg", w.ptu)).exists(), "the user.cfg did not go over either");
    assert!(!Path::new(&format!("{}/USER", w.ptu)).exists());

    // An invalid line in the user.cfg of the source stops the controls before it, too.
    let w = world("zeile", 7200);
    fill_live(&w);
    put(&format!("{}/user.cfg", w.live), "this is not a command line\n");
    let error = copy(&w.paths, &w.live, &w.ptu, &PARTS).expect_err("refused");
    assert!(matches!(error, CopyError::Damaged(Part::UserCfg, _)), "{error:?}");
    assert!(!Path::new(&format!("{}/USER", w.ptu)).exists());
}

#[test]
fn a_part_the_source_does_not_have_is_named_and_the_same_folder_and_no_choice_are_refused() {
    let w = world("leer", 7200);
    put(&format!("{}/user.cfg", w.live), "sys_maxfps = 90\n");
    let error = copy(&w.paths, &w.live, &w.ptu, &[Part::Controls]).expect_err("nothing to give");
    assert!(matches!(error, CopyError::NothingToGive(Part::Controls)), "{error:?}");
    assert!(matches!(copy(&w.paths, &w.live, &w.live, &PARTS), Err(CopyError::SameFolder)));
    assert!(matches!(copy(&w.paths, &w.live, &w.ptu, &[]), Err(CopyError::NothingChosen)));
    let have = available(&w.paths, &w.live);
    assert_eq!(have, Available { graphics: false, controls: false, user_cfg: true });
    assert!(have.has(Part::UserCfg) && !have.has(Part::Graphics));
    // Only the chosen part is written.
    copy(&w.paths, &w.live, &w.ptu, &[Part::UserCfg]).expect("copied");
    assert!(read(&format!("{}/user.cfg", w.ptu)).contains("sys_maxfps = 90"));
    assert!(!Path::new(&format!("{}/USER", w.ptu)).exists());
}
