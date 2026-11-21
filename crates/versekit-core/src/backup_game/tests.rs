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

//! Only throwaway folders: a Linux-style home with a data folder, a LIVE and a PTU folder under it. The installation
//! of the machine is never touched: the game folder is checked to be the test one before anything is written.

use super::*;
use crate::game_start;
use crate::paths::{ENV_INSTALL_DIR, Host};

const ACTIONMAPS: &str = "<ActionMaps><actionmap name=\"seat_general\"/></ActionMaps>";
const ACTIONMAPS_NEW: &str = "<ActionMaps><actionmap name=\"changed\"/></ActionMaps>";
const PROFILE: &str = "<ActionMaps><actionmap name=\"profile_a\"/></ActionMaps>";
const ATTRIBUTES: &str = "<Attributes><Attr name=\"FOV\" value=\"80\"/></Attributes>";
const TEMPLATE: &[u8] = b"CHF\x00\x01template-bytes";

struct World {
    home: String,
    paths: Paths,
    data: String,
    live: String,
    ptu: String,
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

/// A user whose game is quiet (`age` seconds since the log was written; `5`: it runs), with something of their own in
/// the data folder.
fn world(name: &str, age: u64) -> World {
    let home = crate::testdir::fresh(&format!("backupgame-{name}")).to_string_lossy().replace('\\', "/");
    let data = format!("{home}/daten");
    let live = format!("{home}/StarCitizen/LIVE");
    let ptu = format!("{home}/StarCitizen/PTU");
    for folder in [&data, &live, &ptu] {
        std::fs::create_dir_all(folder).expect("folder");
    }
    std::fs::write(format!("{data}/bestand.json"), "{\"a\": 1}").expect("own data");
    std::fs::write(format!("{live}/Game.log"), "x").expect("log");
    let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
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
    // Never the installation of the machine.
    assert_eq!(paths.game_folder().as_deref(), Some(live.as_str()));
    assert_eq!(paths.app_folder(), data);
    World { home, paths, data, live, ptu }
}

fn put(path: &str, content: &[u8]) {
    std::fs::create_dir_all(Path::new(path).parent().expect("parent")).expect("folder");
    std::fs::write(path, content).expect("file");
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn active(folder: &str) -> String {
    format!("{folder}/USER/Client/0/Profiles/default/actionmaps.xml")
}

fn profile(folder: &str, name: &str) -> String {
    format!("{folder}/USER/Client/0/controls/mappings/{name}")
}

fn template(folder: &str, name: &str) -> String {
    format!("{folder}/user/client/0/CustomCharacters/{name}")
}

/// Both channels with everything: active binding, game settings, a profile, a `user.cfg`, a character template.
fn fill(w: &World) {
    for (folder, language) in [(&w.live, "german_(germany)"), (&w.ptu, "english")] {
        put(&active(folder), ACTIONMAPS.as_bytes());
        put(&format!("{folder}/USER/Client/0/Profiles/default/attributes.xml"), ATTRIBUTES.as_bytes());
        put(&profile(folder, "profile_a.xml"), PROFILE.as_bytes());
        put(&format!("{folder}/user.cfg"), format!("g_language = {language}\nsys_maxfps = 90\n").as_bytes());
        put(&template(folder, "Pilotin.chf"), TEMPLATE);
    }
}

/// A backup of everything, as the page writes it.
fn backup_of(w: &World) -> String {
    let target = format!("{}/sicherung.zip", w.home);
    backup::write(&w.paths, &target, "5.4.0", &bindings_all(&w.paths)).expect("written");
    target
}

fn names_in(archive: &str) -> Vec<String> {
    let data = std::fs::read(archive).expect("archive");
    zip::entries(&data).expect("zip").into_iter().map(|e| e.name).collect()
}

/// Every installed channel brings its `user.cfg`, its character templates and its controls along under its own name;
/// the channel that is set up stands the old way too, so that older versions still find its controls.
#[test]
fn a_backup_holds_the_files_of_the_game_of_every_channel_and_the_old_way_of_the_set_up_one() {
    let w = world("alle", 7200);
    fill(&w);
    let names = names_in(&backup_of(&w));
    for channel in ["LIVE", "PTU"] {
        for rest in [
            "actionmaps.xml",
            "attributes.xml",
            "mappings/profile_a.xml",
            "user.cfg",
            "CustomCharacters/Pilotin.chf",
        ] {
            let name = format!("{CHANNELS_PREFIX}{channel}/{rest}");
            assert!(names.contains(&name), "{name} missing in {names:?}");
        }
    }
    for old in ["Steuerung/actionmaps.xml", "Steuerung/attributes.xml", "Steuerung/mappings/profile_a.xml"] {
        assert!(names.contains(&old.to_string()), "{old} missing in {names:?}");
    }
    // What lies there is told by channel.
    let contents = in_archive(&backup_of(&w));
    assert_eq!(contents.iter().map(|c| c.channel.as_str()).collect::<Vec<_>>(), ["LIVE", "PTU"]);
    assert!(contents.iter().all(|c| c.active && c.game_settings && c.user_cfg));
    assert!(contents.iter().all(|c| c.profiles == ["profile_a"] && c.templates == ["Pilotin"]));
}

/// The way of older versions: the restore of the data writes nothing of `Steuerung/` into the data folder, and the
/// controls they read are the ones of the set-up channel only.
#[test]
fn an_older_version_reads_only_the_old_way_and_never_writes_the_channels_into_the_data_folder() {
    let w = world("aelter", 7200);
    fill(&w);
    let archive = backup_of(&w);
    let (active_in, profiles) = backup::bindings_in_archive(&archive);
    assert!(active_in);
    assert_eq!(profiles, ["profile_a"], "the channels are not counted twice");
    backup::restore(&w.paths, &archive, &[]).expect("restored");
    assert!(!Path::new(&format!("{}/Steuerung", w.data)).exists(), "nothing of the game lands in the data folder");
    assert!(!Path::new(&format!("{}/Kanaele", w.data)).exists());
}

/// First yes: profiles and templates come back, the active state stays. Second yes: the active binding, the game
/// settings and the `user.cfg` too, with a copy of the old ones and the language line of the target kept.
#[test]
fn what_only_adds_comes_back_alone_and_the_active_state_only_when_asked_with_copies_and_the_own_language() {
    let w = world("zurueck", 7200);
    fill(&w);
    let archive = backup_of(&w);
    // Afterwards the player lost the profile and the template of PTU and changed the active state of LIVE.
    std::fs::remove_file(profile(&w.ptu, "profile_a.xml")).expect("gone");
    std::fs::remove_file(template(&w.ptu, "Pilotin.chf")).expect("gone");
    put(&active(&w.live), ACTIONMAPS_NEW.as_bytes());
    put(&format!("{}/user.cfg", w.live), b"g_language = english\nsys_maxfps = 30\n");

    let done = restore(&w.paths, &archive, false).expect("restored");
    assert_eq!(read(&profile(&w.ptu, "profile_a.xml")), PROFILE);
    assert_eq!(std::fs::read(template(&w.ptu, "Pilotin.chf")).expect("template"), TEMPLATE);
    assert_eq!(read(&active(&w.live)), ACTIONMAPS_NEW, "the active binding stays without the second yes");
    assert!(read(&format!("{}/user.cfg", w.live)).contains("sys_maxfps = 30"));
    assert!(done.missing.is_empty() && done.count > 0, "{done:?}");

    restore(&w.paths, &archive, true).expect("restored");
    assert_eq!(read(&active(&w.live)), ACTIONMAPS);
    let copies = game_files::backups(&active(&w.live));
    assert_eq!(copies.len(), 1, "{copies:?}");
    assert_eq!(read(&copies[0]), ACTIONMAPS_NEW, "the replaced binding has a copy");
    let cfg = read(&format!("{}/user.cfg", w.live));
    assert!(cfg.contains("sys_maxfps = 90"), "{cfg}");
    assert!(cfg.contains("g_language = english") && !cfg.contains("german"), "the language line stays: {cfg}");
}

/// A channel of the backup that is not installed here is named and nothing of it is written anywhere else.
#[test]
fn a_channel_that_is_not_installed_here_is_named_and_written_nowhere() {
    let w = world("fehlt", 7200);
    fill(&w);
    let archive = backup_of(&w);
    std::fs::remove_dir_all(&w.ptu).expect("PTU gone");
    std::fs::remove_file(profile(&w.live, "profile_a.xml")).expect("gone");
    let done = restore(&w.paths, &archive, true).expect("restored");
    assert_eq!(done.missing, ["PTU"]);
    assert!(!Path::new(&w.ptu).exists(), "no folder of the missing channel is made");
    assert_eq!(read(&profile(&w.live, "profile_a.xml")), PROFILE, "the installed one comes back");
    let preview = preview(&w.paths, &archive);
    let ptu = preview.iter().find(|c| c.channel == "PTU").expect("PTU in the look");
    assert!(!ptu.installed);
    assert!(ptu.additive.iter().chain(&ptu.active).all(|item| item.change == Change::NoPlace), "{ptu:?}");
}

/// While the game runs nothing of the game is written, not even what only adds.
#[test]
fn nothing_of_the_game_is_written_while_it_runs() {
    let w = world("laeuft", 7200);
    fill(&w);
    let archive = backup_of(&w);
    std::fs::remove_file(profile(&w.ptu, "profile_a.xml")).expect("gone");
    // The log is written right now: the game runs.
    std::fs::write(format!("{}/Game.log", w.live), "y").expect("log written now");
    assert!(matches!(restore(&w.paths, &archive, true), Err(BindingsError::GameNotQuiet(_))));
    assert!(!Path::new(&profile(&w.ptu, "profile_a.xml")).exists());
}

/// A name from the archive must not lead out of its folder, and what is no binding or no valid `user.cfg` is left out.
#[test]
fn names_that_lead_out_and_files_that_are_not_fit_are_left_out() {
    let w = world("fremd", 7200);
    fill(&w);
    let archive = format!("{}/fremd.zip", w.home);
    let mut builder = zip::Builder::new((2026, 11, 19, 12, 0, 0));
    builder.add(backup::INFO_FILE, format!("{}\r\n", backup::MARKER).as_bytes()).expect("info");
    builder.add("bestand.json", b"{}").expect("data");
    for (name, content) in [
        ("Steuerung/Kanaele/LIVE/CustomCharacters/../../../evil.chf", TEMPLATE),
        ("Steuerung/Kanaele/LIVE/CustomCharacters/start.exe", TEMPLATE),
        ("Steuerung/Kanaele/LIVE/mappings/kein.xml", b"<NotActionMaps/>".as_slice()),
        ("Steuerung/Kanaele/LIVE/user.cfg", b"this is no line of a cfg\n".as_slice()),
        ("Steuerung/Kanaele/LIVE/mappings/gut.xml", PROFILE.as_bytes()),
    ] {
        builder.add(name, content).expect("member");
    }
    put(&archive, &builder.finish().expect("zip"));
    let before_cfg = read(&format!("{}/user.cfg", w.live));
    let done = restore(&w.paths, &archive, true).expect("restored");
    assert_eq!(done.count, 1, "only the good profile: {done:?}");
    assert_eq!(done.skipped, 4, "{done:?}");
    assert_eq!(read(&profile(&w.live, "gut.xml")), PROFILE);
    assert!(!Path::new(&format!("{}/evil.chf", w.home)).exists());
    assert!(!Path::new(&format!("{}/StarCitizen/evil.chf", w.home)).exists());
    assert!(!Path::new(&format!("{}/evil.chf", w.live)).exists());
    assert!(!Path::new(&format!("{}/user/evil.chf", w.live)).exists());
    assert_eq!(read(&format!("{}/user.cfg", w.live)), before_cfg, "an invalid user.cfg is not written");
}

/// A backup with the controls the old way only (5.0 to 5.4) goes the old way: nothing by channel, the active binding
/// of the set-up channel only on the second yes.
#[test]
fn a_backup_of_the_old_way_is_put_back_the_old_way() {
    let w = world("alt", 7200);
    fill(&w);
    let archive = format!("{}/alt.zip", w.home);
    backup::write(&w.paths, &archive, "5.3.0", &backup::bindings(&w.paths, None)).expect("written");
    assert!(in_archive(&archive).is_empty());
    put(&active(&w.live), ACTIONMAPS_NEW.as_bytes());
    std::fs::remove_file(profile(&w.live, "profile_a.xml")).expect("gone");
    restore(&w.paths, &archive, false).expect("restored");
    assert_eq!(read(&profile(&w.live, "profile_a.xml")), PROFILE);
    assert_eq!(read(&active(&w.live)), ACTIONMAPS_NEW);
    let look = preview(&w.paths, &archive);
    assert_eq!(look.len(), 1, "{look:?}");
    assert_eq!(look[0].channel, "LIVE");
    restore(&w.paths, &archive, true).expect("restored");
    assert_eq!(read(&active(&w.live)), ACTIONMAPS);
}

/// The look before a restore says, file by file, what would be new, replaced or stay the same, for the data and for
/// the game, and it changes nothing.
#[test]
fn the_look_before_a_restore_tells_new_replaced_and_same_and_writes_nothing() {
    let w = world("vorschau", 7200);
    fill(&w);
    let archive = backup_of(&w);
    // Changed afterwards: one data file, the active binding of LIVE; gone: the template of PTU; a new data file.
    std::fs::write(format!("{}/bestand.json", w.data), "{\"a\": 2}").expect("changed");
    std::fs::write(format!("{}/nur-hier.json", w.data), "{}").expect("new here");
    put(&active(&w.live), ACTIONMAPS_NEW.as_bytes());
    std::fs::remove_file(template(&w.ptu, "Pilotin.chf")).expect("gone");

    let data = backup::preview_data(&w.paths, &archive).expect("a backup");
    let bestand = data.iter().find(|item| item.name == "bestand.json").expect("in the look");
    assert_eq!(bestand.change, Change::Replaced);
    assert!(!data.iter().any(|item| item.name == "nur-hier.json"), "a file not in the backup is not touched");
    assert!(!data.iter().any(|item| item.name.starts_with("Steuerung")), "{data:?}");

    let game = preview(&w.paths, &archive);
    let live = game.iter().find(|c| c.channel == "LIVE").expect("LIVE");
    let ptu = game.iter().find(|c| c.channel == "PTU").expect("PTU");
    let change = |items: &[PreviewItem], name: &str| items.iter().find(|i| i.name == name).map(|i| i.change);
    assert_eq!(change(&live.active, "actionmaps.xml"), Some(Change::Replaced));
    assert_eq!(change(&live.active, "user.cfg"), Some(Change::Same));
    assert_eq!(change(&live.additive, "mappings/profile_a.xml"), Some(Change::Same));
    assert_eq!(change(&ptu.additive, "CustomCharacters/Pilotin.chf"), Some(Change::New));
    // Nothing was written by the look.
    assert_eq!(read(&active(&w.live)), ACTIONMAPS_NEW);
    assert!(!Path::new(&template(&w.ptu, "Pilotin.chf")).exists());
    assert_eq!(read(&format!("{}/bestand.json", w.data)), "{\"a\": 2}");
    assert!(backup::preview_data(&w.paths, &format!("{}/bestand.json", w.data)).is_none(), "no backup, no look");
}
