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

//! Every test works on a machine of its own in a throwaway folder: home, Documents and configuration folder are
//! set below it. The real Documents and `%APPDATA%` are never reached.

use std::fs;

use super::*;
use crate::legacy;
use crate::paths::Host;
use crate::pypath::Flavor;

fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("versekit-ablage-fehlt-{name}-{}", crate::testdir::run_tag()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().into_owned()
}

fn machine(root: &str) -> Paths {
    machine_on(root, |host| host)
}

/// [`machine`] with its host changed by `change`.
fn machine_on(root: &str, change: impl FnOnce(Host) -> Host) -> Paths {
    let f = Flavor::native();
    let home = f.join(root, "home");
    let config = f.join(root, "appdata");
    fs::create_dir_all(f.join(&home, "Dokumente")).unwrap();
    fs::create_dir_all(&config).unwrap();
    let vars = [("USERPROFILE", home.as_str()), ("HOME", home.as_str()), ("APPDATA", config.as_str()), ("XDG_CONFIG_HOME", config.as_str())];
    Paths::new(change(Host::new(f, &vars, root)))
}

fn put(path: &str, text: &str) {
    fs::create_dir_all(Flavor::native().dirname(path)).unwrap();
    fs::write(path, text).unwrap();
}

fn json_text(path: &str) -> String {
    format!("\"{}\"", path.replace('\\', "\\\\"))
}

fn documents(root: &str) -> String {
    Flavor::native().join_all(root, &["home", "Dokumente"])
}

/// A data folder with the mark of the program: a settings file with two keys of the template.
fn data_folder(folder: &str) {
    let f = Flavor::native();
    put(&f.join_all(folder, &["Einstellungen", SETTINGS_FILE]), "{\"sprache\": \"de\", \"signalton\": true}");
    put(&f.join_all(folder, &["Bauplaene", "bestand.json"]), "{\"version\": 3, \"bauplaene\": {}}");
}

/// Both pointers name `target`, the second one of the current name only.
fn point_to(p: &Paths, target: &str) {
    let (second, _) = p.second_pointer_places();
    put(&second, &format!("{{\"ablage_ordner\": {}}}", json_text(target)));
}

/// A pointer to a folder that is gone and no data folder anywhere else: no empty folder appears, nothing can be
/// written below the place, and the player is told which place it was.
#[test]
fn a_pointer_into_nothing_makes_no_empty_folder_and_says_so() {
    let root = scratch("nichts");
    let p = machine(&root);
    let f = Flavor::native();
    let weg = f.join(&root, "Weg");
    point_to(&p, &weg);

    assert_eq!(p.app_folder(), weg, "the place the pointer names, left as it is");
    assert!(fs::metadata(&weg).is_err(), "no new, empty folder");
    assert!(!p.set_setting("sprache", Value::str("en")), "nothing is written below it");
    assert!(fs::metadata(&weg).is_err(), "still no folder after a write");
    let _ = p.app_file("bestand.json");
    assert!(fs::metadata(&weg).is_err(), "asking for an own file makes none");

    let lost = p.storage_lost().expect("the player is told");
    assert_eq!(lost, Lost { missing: weg.clone(), found: Vec::new(), unreachable: None });
    let de = p.lost_message_in(language::Lang::De).unwrap();
    let en = p.lost_message_in(language::Lang::En).unwrap();
    assert!(de.contains(&weg) && en.contains(&weg), "{de} / {en}");
    assert_ne!(de, en);
    let _ = fs::remove_dir_all(&root);
}

/// One data folder at a known place: it is taken, and the pointers name it from then on.
#[test]
fn exactly_one_data_folder_found_is_taken_and_the_pointers_healed() {
    let root = scratch("einer");
    let p = machine(&root);
    let f = Flavor::native();
    let weg = f.join(&root, "Weg");
    let there = f.join(&documents(&root), NEW_FOLDER_NAME);
    data_folder(&there);
    point_to(&p, &weg);

    assert_eq!(p.app_folder(), there);
    assert!(fs::metadata(&weg).is_err(), "no new, empty folder at the old place");
    assert_eq!(p.storage_lost(), None);
    let (second, _) = p.second_pointer_places();
    assert_eq!(p.read_pointer(&second).as_deref(), Some(there.as_str()), "the pointer names the data now");
    let settings = fs::read_to_string(f.join_all(&there, &["Einstellungen", SETTINGS_FILE])).unwrap();
    assert!(settings.contains("\"sprache\": \"de\""), "the settings of the folder stay: {settings}");
    assert_eq!(machine(&root).app_folder(), there, "the next start finds it without a search");
    let _ = fs::remove_dir_all(&root);
}

/// A folder far from Documents that a pointer names counts as well, and an empty folder under the current name
/// does not take the place of it.
#[test]
fn a_data_folder_that_another_pointer_names_is_found_and_an_empty_one_is_not_taken() {
    let root = scratch("zeiger");
    let p = machine(&root);
    let f = Flavor::native();
    let (weg, other) = (f.join(&root, "Weg"), f.join(&root, "Andere"));
    data_folder(&other);
    fs::create_dir_all(f.join(&documents(&root), NEW_FOLDER_NAME)).unwrap();
    let (second, second_old) = p.second_pointer_places();
    put(&second, &format!("{{\"ablage_ordner\": {}}}", json_text(&weg)));
    put(&second_old, &format!("{{\"ablage_ordner\": {}}}", json_text(&other)));

    assert_eq!(p.app_folder(), other);
    assert!(fs::metadata(&weg).is_err());
    assert_eq!(p.read_pointer(&second).as_deref(), Some(other.as_str()));
    let _ = fs::remove_dir_all(&root);
}

/// Two data folders: the program does not choose; it names both and creates nothing.
#[test]
fn several_data_folders_are_named_and_none_is_chosen() {
    let root = scratch("mehrere");
    let p = machine(&root);
    let f = Flavor::native();
    let weg = f.join(&root, "Weg");
    let (new, old) = (f.join(&documents(&root), NEW_FOLDER_NAME), f.join(&documents(&root), legacy::DOCUMENTS_FOLDER));
    data_folder(&new);
    data_folder(&old);
    point_to(&p, &weg);

    assert_eq!(p.app_folder(), weg);
    assert!(fs::metadata(&weg).is_err());
    let lost = p.storage_lost().expect("told");
    assert_eq!(lost.missing, weg);
    assert_eq!(lost.found.len(), 2, "{lost:?}");
    assert!(lost.found.contains(&new) && lost.found.contains(&old));
    let text = p.lost_message_in(language::Lang::De).unwrap();
    assert!(text.contains(&new) && text.contains(&old), "{text}");
    let _ = fs::remove_dir_all(&root);
}

/// The place is there again (the disk is connected): it is used as before, and writing below it works.
#[test]
fn the_place_that_comes_back_is_used_again() {
    let root = scratch("zurueck");
    let p = machine(&root);
    let f = Flavor::native();
    let weg = f.join(&root, "Weg");
    point_to(&p, &weg);
    assert_eq!(p.app_folder(), weg);
    assert!(p.storage_lost().is_some());

    fs::create_dir_all(&weg).unwrap();
    assert!(p.set_setting("sprache", Value::str("en")), "a folder that is there is written to");
    assert_eq!(p.storage_lost(), None);
    assert!(fs::metadata(f.join_all(&weg, &["Einstellungen", SETTINGS_FILE])).is_ok());
    let _ = fs::remove_dir_all(&root);
}

/// A machine that never had a data folder gets its default folder, as before: only a pointer into nothing is
/// not made good.
#[test]
fn a_fresh_machine_still_gets_its_default_folder() {
    let root = scratch("frisch");
    let p = machine(&root);
    let folder = p.app_folder();
    assert_eq!(folder, p.default_folder());
    assert!(fs::metadata(&folder).is_ok_and(|m| m.is_dir()));
    assert_eq!(p.storage_lost(), None);
    let _ = fs::remove_dir_all(&root);
}

/// The texts of every pointer file of the machine, to see that none was written.
fn pointer_texts(p: &Paths) -> Vec<Option<String>> {
    let (second, second_old) = p.second_pointer_places();
    [p.pointer_file(), second, second_old].iter().map(|file| fs::read_to_string(file).ok()).collect()
}

/// The drive of the data folder is not ready (a check of the disk at start) and one data folder with the mark lies
/// under Documents under an earlier name: it is not taken, no pointer is written, nothing is made, and the player
/// is told the drive cannot be reached.
#[test]
fn a_drive_that_is_not_ready_is_never_traded_for_another_data_folder() {
    let root = scratch("nicht-bereit");
    let p = machine_on(&root, |host| host.with_unready_roots(usize::MAX));
    let f = Flavor::native();
    let weg = f.join(&root, "Weg");
    let earlier = f.join(&documents(&root), "Verse-Kit");
    data_folder(&earlier);
    point_to(&p, &weg);
    let before = pointer_texts(&p);

    assert_eq!(p.app_folder(), weg, "the place the pointer names, never the folder under Documents");
    assert_eq!(pointer_texts(&p), before, "no pointer is written");
    assert!(fs::metadata(&weg).is_err(), "nothing is made");
    assert!(!p.set_setting("sprache", Value::str("en")), "nothing is written below it");
    assert!(fs::metadata(&weg).is_err());
    assert!(p.settings_file().starts_with(&weg), "the settings are looked for at the place the pointer names");

    let lost = p.storage_lost().expect("the player is told");
    assert_eq!(lost.missing, weg);
    assert!(lost.found.is_empty(), "no other place is looked for: {lost:?}");
    let drive = storage_root(f, &weg).expect("a root");
    assert_eq!(lost.unreachable.as_deref(), Some(drive.as_str()));
    let de = p.lost_message_in(language::Lang::De).unwrap();
    let en = p.lost_message_in(language::Lang::En).unwrap();
    assert!(de.contains(&weg) && en.contains(&weg) && de.contains(&drive), "{de} / {en}");
    assert_ne!(de, en);
    assert_eq!(de, language::tf_in(language::Lang::De, "ablage_laufwerk_fehlt", &[("0", Arg::from(weg.as_str())), ("1", Arg::from(drive.as_str()))]));

    // A start after it, the drive still not ready: the same.
    let next = machine_on(&root, |host| host.with_unready_roots(usize::MAX));
    assert_eq!(next.app_folder(), weg);
    assert_eq!(pointer_texts(&next), before);
    let settings = fs::read_to_string(f.join_all(&earlier, &["Einstellungen", SETTINGS_FILE])).unwrap();
    assert!(!settings.contains(STORAGE_KEY), "the folder under Documents names no place now either: {settings}");
    let _ = fs::remove_dir_all(&root);
}

/// The drive answers after a few questions within the wait, and the folder is really not there: then the one data
/// folder found is taken, as for any folder that is gone.
#[test]
fn a_drive_that_gets_ready_while_waited_for_counts_as_answering() {
    let root = scratch("wird-bereit");
    let p = machine_on(&root, |host| host.with_unready_roots(2));
    let f = Flavor::native();
    let weg = f.join(&root, "Weg");
    let there = f.join(&documents(&root), NEW_FOLDER_NAME);
    data_folder(&there);
    point_to(&p, &weg);

    assert_eq!(p.app_folder(), there);
    let (second, _) = p.second_pointer_places();
    assert_eq!(p.read_pointer(&second).as_deref(), Some(there.as_str()));
    let _ = fs::remove_dir_all(&root);
}

/// The visible pointer names a folder on a drive that is not ready, the second one another folder that is there:
/// the other one is not taken in its place and the visible one is not written over with it.
#[test]
fn the_other_pointer_does_not_step_in_for_a_drive_that_is_not_ready() {
    let root = scratch("anderer-zeiger");
    let p = machine_on(&root, |host| host.with_unready_roots(usize::MAX));
    let f = Flavor::native();
    let (weg, other) = (f.join(&root, "Weg"), f.join(&root, "Andere"));
    data_folder(&other);
    put(&p.pointer_file(), &format!("{{\"ablage_ordner\": {}}}", json_text(&weg)));
    point_to(&p, &other);
    let before = pointer_texts(&p);

    assert_eq!(p.app_folder(), weg);
    assert_eq!(pointer_texts(&p), before, "no pointer is written");
    assert!(p.storage_lost().is_some_and(|lost| lost.unreachable.is_some()));
    let _ = fs::remove_dir_all(&root);
}

/// A data folder on a drive letter the computer does not have: the real question to the system says it does not
/// answer, and the one data folder under Documents is not taken.
#[cfg(windows)]
#[test]
fn a_drive_letter_that_is_not_there_is_not_traded_either() {
    let Some(letter) = ('D'..='Z').rev().find(|l| fs::metadata(format!("{l}:\\")).is_err()) else { return };
    let root = scratch("kein-laufwerk");
    let p = machine(&root);
    let f = Flavor::native();
    let weg = format!("{letter}:\\VerseKit-Daten");
    data_folder(&f.join(&documents(&root), NEW_FOLDER_NAME));
    point_to(&p, &weg);
    let before = pointer_texts(&p);

    assert_eq!(p.app_folder(), weg);
    assert_eq!(pointer_texts(&p), before);
    let lost = p.storage_lost().expect("told");
    assert_eq!(lost.unreachable, Some(format!("{letter}:\\")));
    let _ = fs::remove_dir_all(&root);
}

/// The roots that tell a drive that is gone from a folder that is gone.
#[test]
fn the_root_of_a_place_is_its_drive_share_or_mount_point() {
    let (w, x) = (Flavor::Windows, Flavor::Posix);
    assert_eq!(storage_root(w, "E:\\Spiele\\VerseKit-Daten").as_deref(), Some("E:\\"));
    assert_eq!(storage_root(w, "e:/daten").as_deref(), Some("e:\\"));
    assert_eq!(storage_root(w, "\\\\nas\\freigabe\\VerseKit").as_deref(), Some("\\\\nas\\freigabe\\"));
    assert_eq!(storage_root(w, "daten"), None);
    assert_eq!(storage_root(x, "/media/spieler/Platte/VerseKit").as_deref(), Some("/media/spieler/Platte"));
    assert_eq!(storage_root(x, "/run/media/spieler/Platte/VerseKit").as_deref(), Some("/run/media/spieler/Platte"));
    assert_eq!(storage_root(x, "/mnt/spiele/VerseKit").as_deref(), Some("/mnt/spiele"));
    assert_eq!(storage_root(x, "/Volumes/Spiele/VerseKit").as_deref(), Some("/Volumes/Spiele"));
    assert_eq!(storage_root(x, "/home/spieler/VerseKit"), None);
}
