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
//! set below it, the registry is not asked. The real Documents and `%APPDATA%` are never reached.

use std::fs;

use super::*;
use crate::legacy;
use crate::paths::{CONFIG_FOLDER_NAME, Host, SECOND_POINTER_FILE};
use crate::pypath::Flavor;

fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("versekit-namensumzug-{name}-{}", crate::testdir::run_tag()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().into_owned()
}

fn vars(root: &str) -> Vec<(String, String)> {
    let f = Flavor::native();
    let home = f.join(root, "home");
    let config = f.join(root, "appdata");
    fs::create_dir_all(f.join(&home, "Dokumente")).unwrap();
    fs::create_dir_all(&config).unwrap();
    vec![
        ("USERPROFILE".into(), home.clone()),
        ("HOME".into(), home),
        ("APPDATA".into(), config.clone()),
        ("XDG_CONFIG_HOME".into(), config),
    ]
}

/// A machine of its own below `root`, with the variables `extra` besides.
fn machine_with(root: &str, extra: &[(&str, &str)]) -> Paths {
    let mut all = vars(root);
    all.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    let refs: Vec<(&str, &str)> = all.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    Paths::new(Host::new(Flavor::native(), &refs, root))
}

fn machine(root: &str) -> Paths {
    machine_with(root, &[])
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

/// The data of an installation of 4.12 under Documents: settings, collection, the second pointer and a lock file
/// next to it.
struct Old {
    folder: String,
    new: String,
    settings: String,
    collection: String,
}

fn old_installation(root: &str, name: &str) -> Old {
    let f = Flavor::native();
    let folder = f.join(&documents(root), name);
    let new = f.join(&documents(root), NEW_FOLDER_NAME);
    let settings = f.join_all(&folder, &["Einstellungen", SETTINGS_FILE]);
    let export = f.join(&folder, "export");
    put(&settings, &format!("{{\"sprache\": \"de\", \"export_ordner\": {}, \"zahl\": 3}}", json_text(&export)));
    let collection = f.join_all(&folder, &["Bauplaene", "bestand.json"]);
    put(&collection, "{\"bp\": [1, 2, 3]}");
    let config = f.join_all(root, &["appdata", legacy::CONFIG_FOLDER]);
    put(&f.join(&config, "sperre-abc.lock"), "");
    Old { folder, new, settings, collection }
}

#[test]
fn the_safe_case_renames_once_and_the_second_start_does_nothing() {
    let root = scratch("sicher");
    let p = machine(&root);
    let f = Flavor::native();
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    let collection_before = fs::read(&old.collection).unwrap();
    assert_eq!(p.default_folder(), old.folder, "the old one is the default folder before");
    assert_eq!(p.folder_to_rename(), Some((old.folder.clone(), old.new.clone())));
    let preview = p.rename_preview();
    assert_eq!(preview, Renamed::Stays { folder: old.folder.clone(), target: old.new.clone(), why: Hold::NotYet });
    assert!(preview.can_retry() && fs::metadata(&old.folder).is_ok(), "the preview renames nothing");

    assert_eq!(p.rename_to_current_name(), Renamed::Done { from: old.folder.clone(), to: old.new.clone() });
    assert!(fs::metadata(&old.folder).is_err(), "renamed, not copied");
    assert_eq!(fs::read(f.join_all(&old.new, &["Bauplaene", "bestand.json"])).unwrap(), collection_before);
    let settings = fs::read_to_string(f.join_all(&old.new, &["Einstellungen", SETTINGS_FILE])).unwrap();
    assert!(settings.contains(&json_text(&f.join(&old.new, "export"))), "the place in it follows: {settings}");
    assert!(settings.contains("\"zahl\": 3") && settings.contains("\"sprache\": \"de\""), "{settings}");
    assert_eq!(p.default_folder(), old.new);
    assert_eq!(p.app_folder(), old.new);
    assert!(fs::metadata(&old.folder).is_err(), "asking for the data folder makes no old one");
    let config_old = f.join_all(&root, &["appdata", legacy::CONFIG_FOLDER]);
    assert_eq!(
        p.read_pointer(&f.join(&config_old, SECOND_POINTER_FILE)).as_deref(),
        Some(old.new.as_str()),
        "the old configuration folder names the renamed folder: an older version started later finds it"
    );
    assert!(p.second_pointer().contains(CONFIG_FOLDER_NAME), "the new one is the one in use");

    let again = machine(&root);
    assert_eq!(again.folder_to_rename(), None);
    assert_eq!(again.rename_preview(), Renamed::Nothing, "the page has nothing to say any more");
    assert_eq!(again.rename_to_current_name(), Renamed::Nothing, "the second start does nothing");
    assert_eq!(again.app_folder(), old.new);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn the_folder_of_5_0_is_renamed_too() {
    let root = scratch("fuenf");
    let p = machine(&root);
    let old = old_installation(&root, EARLIER_FOLDER_NAMES[1]);
    assert_eq!(EARLIER_FOLDER_NAMES[1], "Verse-Kit");
    assert!(matches!(p.rename_to_current_name(), Renamed::Done { .. }));
    assert!(fs::metadata(&old.folder).is_err() && fs::metadata(&old.new).is_ok());
    let _ = fs::remove_dir_all(&root);
}

/// A data folder elsewhere that lies inside the old folder, with both pointers naming it, and data of an older
/// version next to the second pointer: the pointers follow together, the old data stay where they were.
#[test]
fn both_pointers_follow_together_and_the_data_of_an_older_version_stay() {
    let root = scratch("zeiger");
    let p = machine(&root);
    let f = Flavor::native();
    let folder = f.join(&documents(&root), legacy::DOCUMENTS_FOLDER);
    let storage = f.join(&folder, "Ablage");
    let first = f.join_all(&folder, &["Einstellungen", SETTINGS_FILE]);
    put(&first, &format!("{{\"ablage_ordner\": {}}}", json_text(&storage)));
    put(&f.join_all(&storage, &["Einstellungen", SETTINGS_FILE]), "{\"sprache\": \"en\"}");
    put(&f.join_all(&storage, &["Bauplaene", "bestand.json"]), "{\"x\": 1}");
    let config_old = f.join_all(&root, &["appdata", legacy::CONFIG_FOLDER]);
    let second_old = f.join(&config_old, SECOND_POINTER_FILE);
    put(&second_old, &format!("{{\"ablage_ordner\": {}}}", json_text(&storage)));
    put(&f.join(&config_old, "bestand.json"), "{\"alt\": true}");
    assert_eq!(p.app_folder(), storage);

    assert!(matches!(p.rename_to_current_name(), Renamed::Done { .. }));
    let new = f.join(&documents(&root), NEW_FOLDER_NAME);
    let moved = f.join(&new, "Ablage");
    let (second, _) = p.second_pointer_places();
    assert_eq!(p.read_pointer(&f.join_all(&new, &["Einstellungen", SETTINGS_FILE])).as_deref(), Some(moved.as_str()));
    assert_eq!(p.read_pointer(&second).as_deref(), Some(moved.as_str()), "the second pointer goes along");
    assert_eq!(
        p.read_pointer(&second_old).as_deref(),
        Some(moved.as_str()),
        "the old second pointer stays and names the new place: an older version started later finds the data"
    );
    assert_eq!(fs::read_to_string(f.join(&config_old, "bestand.json")).unwrap(), "{\"alt\": true}");
    assert_eq!(p.app_folder(), moved);
    assert!(fs::metadata(&storage).is_err(), "no new, empty folder at the old place");
    assert_eq!(p.setting("sprache").as_deref(), Some("en"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn in_onedrive_everything_stays_and_the_hint_says_why() {
    let root = scratch("onedrive");
    let p = machine_with(&root, &[("OneDrive", &root)]);
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    let before = fs::read(&old.settings).unwrap();
    assert!(p.in_onedrive(&old.folder));
    let result = p.rename_to_current_name();
    assert_eq!(result, Renamed::Stays { folder: old.folder.clone(), target: old.new.clone(), why: Hold::OneDrive });
    assert!(!result.can_retry(), "no button for OneDrive");
    assert!(result.message().unwrap().contains(&old.folder));
    assert_eq!(fs::read(&old.settings).unwrap(), before, "untouched");
    assert!(fs::metadata(&old.new).is_err());
    assert_eq!(p.app_folder(), old.folder);
    // A folder named after OneDrive counts as well, without the variable.
    let plain = machine(&root);
    assert!(plain.in_onedrive(&Flavor::native().join_all(&root, &["OneDrive - Firma", "Dokumente"])));
    assert!(!plain.in_onedrive(&old.folder));
    assert!(plain.in_onedrive(&Flavor::native().join_all(&root, &["OneDrive", "Dokumente"])));
    assert!(
        !plain.in_onedrive(&Flavor::native().join_all(&root, &["OneDrive-Archiv", "Dokumente"])),
        "only the names of OneDrive"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_folder_of_the_new_name_that_is_there_keeps_everything_where_it_is() {
    let root = scratch("ziel");
    let p = machine(&root);
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    put(&Flavor::native().join(&old.new, "meins.txt"), "von Hand");
    assert_eq!(p.default_folder(), old.folder, "a folder of the new name without settings does not take over");
    let result = p.rename_to_current_name();
    assert_eq!(result, Renamed::Stays { folder: old.folder.clone(), target: old.new.clone(), why: Hold::TargetThere });
    assert!(result.can_retry());
    assert!(result.message().unwrap().contains(&old.new));
    assert!(fs::metadata(&old.collection).is_ok());
    assert_eq!(fs::read_to_string(Flavor::native().join(&old.new, "meins.txt")).unwrap(), "von Hand");
    assert_eq!(p.app_folder(), old.folder);
    // The start tells it once; the next start keeps quiet.
    assert!(p.rename_notice(&result).is_some());
    assert_eq!(machine(&root).rename_notice(&result), None);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_data_folder_given_from_outside_is_never_renamed() {
    let root = scratch("vorgabe");
    let own = Flavor::native().join(&root, "eigen");
    let p = machine_with(&root, &[(crate::envvars::ENV_HOME, &own)]);
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    assert_eq!(p.folder_to_rename(), None);
    assert_eq!(p.rename_to_current_name(), Renamed::Nothing);
    assert!(fs::metadata(&old.folder).is_ok() && fs::metadata(&old.new).is_err());
    let config_old = Flavor::native().join_all(&root, &["appdata", legacy::CONFIG_FOLDER]);
    assert!(fs::metadata(config_old).is_ok(), "nothing of the player's own is touched");
    let _ = fs::remove_dir_all(&root);
}

/// The program ended right after the rename, before the pointers were written anew: the next start heals them,
/// and no new, empty folder appears at the old place.
#[test]
fn a_rename_cut_off_before_the_pointers_leaves_everything_readable() {
    let root = scratch("abgebrochen");
    let p = machine(&root);
    let f = Flavor::native();
    let docs = documents(&root);
    let (old, new) = (f.join(&docs, legacy::DOCUMENTS_FOLDER), f.join(&docs, NEW_FOLDER_NAME));
    let (storage_old, storage_new) = (f.join(&old, "Ablage"), f.join(&new, "Ablage"));
    // The state as the rename left it: the folder has its new name, both pointers still name the old place.
    put(
        &f.join_all(&new, &["Einstellungen", SETTINGS_FILE]),
        &format!("{{\"ablage_ordner\": {}}}", json_text(&storage_old)),
    );
    put(&f.join_all(&storage_new, &["Einstellungen", SETTINGS_FILE]), "{\"sprache\": \"en\"}");
    put(&f.join_all(&storage_new, &["Bauplaene", "bestand.json"]), "{\"x\": 1}");
    let second_old = f.join_all(&root, &["appdata", legacy::CONFIG_FOLDER, SECOND_POINTER_FILE]);
    put(&second_old, &format!("{{\"ablage_ordner\": {}}}", json_text(&storage_old)));

    assert_eq!(p.rename_to_current_name(), Renamed::Nothing);
    assert_eq!(p.app_folder(), storage_new, "the data are found where they are");
    assert!(fs::metadata(&old).is_err(), "no new, empty folder at the old place");
    assert_eq!(p.setting("sprache").as_deref(), Some("en"));
    let (second, _) = p.second_pointer_places();
    assert_eq!(p.read_pointer(&second).as_deref(), Some(storage_new.as_str()));
    let _ = fs::remove_dir_all(&root);
}

/// A rename the system refuses (a file held open in the folder under Windows) changes nothing: everything reads
/// as before, and the button can try again once the file is closed.
#[cfg(windows)]
#[test]
fn a_refused_rename_leaves_everything_readable_and_the_next_try_works() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = scratch("verweigert");
    let p = machine(&root);
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    let before = fs::read(&old.settings).unwrap();
    let held = fs::OpenOptions::new().read(true).share_mode(0).open(&old.collection).unwrap();
    let result = p.rename_to_current_name();
    assert!(matches!(&result, Renamed::Stays { why: Hold::Refused(_), .. }), "{result:?}");
    assert!(result.can_retry());
    assert_eq!(fs::read(&old.settings).unwrap(), before);
    assert_eq!(p.app_folder(), old.folder);
    assert!(fs::metadata(&old.new).is_err());
    drop(held);
    assert!(matches!(p.rename_to_current_name(), Renamed::Done { .. }));
    let _ = fs::remove_dir_all(&root);
}

/// The same under Linux: a Documents folder without the right to write refuses the rename.
#[cfg(unix)]
#[test]
fn a_refused_rename_leaves_everything_readable_and_the_next_try_works() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("verweigert");
    let p = machine(&root);
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    let before = fs::read(&old.settings).unwrap();
    let docs = documents(&root);
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o555)).unwrap();
    let result = p.rename_to_current_name();
    fs::set_permissions(&docs, fs::Permissions::from_mode(0o755)).unwrap();
    // As root the right is not asked: then there is nothing to check here.
    if matches!(result, Renamed::Done { .. }) {
        let _ = fs::remove_dir_all(&root);
        return;
    }
    assert!(matches!(&result, Renamed::Stays { why: Hold::Refused(_), .. }), "{result:?}");
    assert_eq!(fs::read(&old.settings).unwrap(), before);
    assert_eq!(p.app_folder(), old.folder);
    assert!(matches!(p.rename_to_current_name(), Renamed::Done { .. }));
    let _ = fs::remove_dir_all(&root);
}

/// A rename counts as worked only when the folder of the new name is there, the old one is gone and the settings
/// in it read the same: any other state sends the folder back under its old name.
#[test]
fn a_rename_that_changed_what_is_read_is_no_rename_that_worked() {
    let root = scratch("gelesen");
    let f = Flavor::native();
    let (old, new) = (f.join(&root, "alt"), f.join(&root, "neu"));
    let settings = b"{\"sprache\": \"de\"}".to_vec();
    let (before, other) = (Some(settings.clone()), Some(b"{\"sprache\": \"en\"}".to_vec()));
    fs::create_dir_all(&new).unwrap();
    assert!(rename_left_everything_as_it_was(&old, &new, &before, Some(before.clone())), "read the same");
    assert!(rename_left_everything_as_it_was(&old, &new, &None, None), "no settings before, none looked for");
    assert!(!rename_left_everything_as_it_was(&old, &new, &before, Some(other)), "read differently");
    assert!(!rename_left_everything_as_it_was(&old, &new, &before, Some(None)), "cannot be read any more");
    fs::create_dir_all(&old).unwrap();
    assert!(!rename_left_everything_as_it_was(&old, &new, &before, Some(before.clone())), "the old one is still there");
    fs::remove_dir_all(&new).unwrap();
    assert!(!rename_left_everything_as_it_was(&old, &new, &None, None), "the new one is not there");
    let _ = fs::remove_dir_all(&root);
}

/// The way back: a folder that was renamed gets its old name again, with everything in it.
#[test]
fn a_rename_that_did_not_work_is_taken_back_with_everything_in_it() {
    let root = scratch("zurueck");
    let p = machine(&root);
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    let before = fs::read(&old.collection).unwrap();
    fs::rename(&old.folder, &old.new).unwrap();
    assert!(p.rename_back(&old.new, &old.folder));
    assert!(fs::metadata(&old.new).is_err());
    assert_eq!(fs::read(&old.collection).unwrap(), before);
    assert!(!p.rename_back(&old.new, &old.folder), "nothing to take back is no success");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn a_place_is_moved_only_when_it_lies_in_the_folder() {
    let p = Paths::new(Host::new(Flavor::Windows, &[], "C:\\"));
    let from = format!(r"C:\Users\A\Documents\{}", legacy::DOCUMENTS_FOLDER);
    let (from, to) = (from.as_str(), r"C:\Users\A\Documents\VerseKit");
    let lower = format!(r"{}\export", from.to_lowercase());
    assert_eq!(p.moved_path(&lower, from, to).as_deref(), Some(r"C:\Users\A\Documents\VerseKit\export"));
    assert_eq!(p.moved_path(from, from, to).as_deref(), Some(to));
    assert_eq!(p.moved_path(&format!(r"{from}2\x"), from, to), None, "another folder");
    assert_eq!(p.moved_path(r"D:\Spiele", from, to), None);
}

#[test]
fn every_hint_has_its_words_in_both_languages() {
    let stays = |why: Hold| Renamed::Stays { folder: "A".into(), target: "B".into(), why };
    for result in [
        Renamed::Done { from: "A".into(), to: "B".into() },
        stays(Hold::OneDrive),
        stays(Hold::TargetThere),
        stays(Hold::Busy),
        stays(Hold::Refused("x".into())),
        stays(Hold::NotSame),
        stays(Hold::NotYet),
    ] {
        let de = result.message_in(crate::language::Lang::De).unwrap();
        let en = result.message_in(crate::language::Lang::En).unwrap();
        for text in [&de, &en] {
            assert!(!text.contains("nu_") && (text.contains('A') || text.contains('B')), "{result:?}: {text}");
        }
        assert_ne!(de, en, "{result:?}");
    }
    assert_eq!(Renamed::Nothing.message(), None);
}

/// A storage folder chosen on the page goes to every pointer an older or a newer version may read: the visible
/// one of each folder under Documents that has one, and the second one in the folder of the current name and of
/// the previous version.
#[test]
fn a_new_storage_folder_is_written_to_every_pointer() {
    let root = scratch("alle-zeiger");
    let p = machine(&root);
    let f = Flavor::native();
    let docs = documents(&root);
    let (new_settings, old_settings) = (
        f.join_all(&docs, &[NEW_FOLDER_NAME, "Einstellungen", SETTINGS_FILE]),
        f.join_all(&docs, &[EARLIER_FOLDER_NAMES[1], "Einstellungen", SETTINGS_FILE]),
    );
    put(&new_settings, "{\"sprache\": \"de\"}");
    put(&old_settings, "{\"sprache\": \"en\"}");
    let config_old = f.join_all(&root, &["appdata", legacy::CONFIG_FOLDER, SECOND_POINTER_FILE]);
    put(&config_old, "{\"ablage_ordner\": \"X:\\\\weg\"}");
    let storage = f.join(&root, "Ablage");
    fs::create_dir_all(&storage).unwrap();

    assert!(p.set_setting(crate::paths::STORAGE_KEY, Value::str(&storage)));
    let (second, second_old) = p.second_pointer_places();
    for (name, file) in [
        ("visible, current name", &new_settings),
        ("visible, earlier name", &old_settings),
        ("second, current name", &second),
        ("second, previous version", &second_old),
    ] {
        assert_eq!(p.read_pointer(file).as_deref(), Some(storage.as_str()), "{name}");
    }
    assert!(fs::read_to_string(&new_settings).unwrap().contains("\"sprache\": \"de\""), "settings stay");
    assert!(fs::read_to_string(&old_settings).unwrap().contains("\"sprache\": \"en\""), "settings stay");
    // A folder under an earlier name that is not there is not made for a pointer.
    assert!(fs::metadata(f.join(&docs, legacy::DOCUMENTS_FOLDER)).is_err());
    let _ = fs::remove_dir_all(&root);
}

/// Moving the data to another folder ends in the same state: no pointer keeps the old place.
#[test]
fn a_move_of_the_data_folder_writes_every_pointer_too() {
    let root = scratch("umzug-zeiger");
    let p = machine(&root);
    let f = Flavor::native();
    let old = old_installation(&root, EARLIER_FOLDER_NAMES[1]);
    let config_old = f.join_all(&root, &["appdata", legacy::CONFIG_FOLDER, SECOND_POINTER_FILE]);
    put(&config_old, &format!("{{\"ablage_ordner\": {}}}", json_text(&old.folder)));
    let target = f.join(&root, "Neu");
    assert!(matches!(
        p.apply_storage_move(&target, crate::storage_move::Choice::TakeAlong),
        crate::storage_move::Outcome::Moved { .. }
    ));
    let (second, second_old) = p.second_pointer_places();
    for file in [&p.pointer_file(), &second, &second_old] {
        assert_eq!(p.read_pointer(file).as_deref(), Some(target.as_str()), "{file}");
    }
    assert_eq!(p.app_folder(), target);
    let _ = fs::remove_dir_all(&root);
}

/// The default folder renamed without any pointer before: the second pointer of the previous version is there
/// afterwards and names the new folder, and starting from it does not write over the settings of that folder.
#[test]
fn the_old_second_pointer_names_a_renamed_default_folder_and_the_settings_stay() {
    let root = scratch("alt-zeiger");
    let p = machine(&root);
    let f = Flavor::native();
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    let config_old = f.join_all(&root, &["appdata", legacy::CONFIG_FOLDER]);
    assert!(fs::metadata(f.join(&config_old, SECOND_POINTER_FILE)).is_err(), "none before");
    assert!(matches!(p.rename_to_current_name(), Renamed::Done { .. }));
    assert_eq!(p.read_pointer(&f.join(&config_old, SECOND_POINTER_FILE)).as_deref(), Some(old.new.as_str()));
    let settings = f.join_all(&old.new, &["Einstellungen", SETTINGS_FILE]);
    let before = fs::read_to_string(&settings).unwrap();
    assert!(before.contains("\"zahl\": 3") && before.contains("\"sprache\": \"de\""), "the rename kept them: {before}");
    let later = machine(&root);
    assert_eq!(later.app_folder(), old.new);
    assert_eq!(fs::read_to_string(&settings).unwrap(), before, "a pointer to the default folder heals nothing");
    let _ = fs::remove_dir_all(&root);
}

/// The mark of the highest blueprint count lies next to the second pointer. When the pointer moves to the folder of
/// the current name, the mark is copied there (the old one stays), so that the warning "fewer blueprints than
/// before" holds after the update; a mark that is there already is never written over.
#[test]
fn the_mark_of_the_highest_count_goes_along_with_the_second_pointer() {
    let root = scratch("hochwasser");
    let f = Flavor::native();
    let old = old_installation(&root, legacy::DOCUMENTS_FOLDER);
    let config_old = f.join_all(&root, &["appdata", legacy::CONFIG_FOLDER]);
    let mark_old = f.join(&config_old, crate::collection::HIGH_WATER_FILE);
    let mark_text = "{\"bauplaene\": 412, \"ordner\": \"D:\\\\Daten\", \"stand\": \"2026-11-15\"}";
    put(&mark_old, mark_text);
    put(&f.join(&config_old, SECOND_POINTER_FILE), &format!("{{\"ablage_ordner\": {}}}", json_text(&old.folder)));

    let p = machine(&root);
    assert!(matches!(p.rename_to_current_name(), Renamed::Done { .. }));
    let mark_of = |p: &Paths| f.join(&f.dirname(&p.second_pointer()), crate::collection::HIGH_WATER_FILE);
    let mark_new = mark_of(&p);
    assert!(mark_new.contains(CONFIG_FOLDER_NAME) && !mark_new.contains(legacy::CONFIG_FOLDER), "{mark_new}");
    assert_eq!(fs::read_to_string(&mark_new).unwrap(), mark_text, "the new second pointer has the mark next to it");
    assert_eq!(fs::read_to_string(&mark_old).unwrap(), mark_text, "the old one stays for an older version");

    // A mark the new version wrote itself is not replaced by the old one at the next start.
    put(&mark_new, "{\"bauplaene\": 500}");
    assert_eq!(machine(&root).rename_to_current_name(), Renamed::Nothing);
    assert_eq!(fs::read_to_string(&mark_new).unwrap(), "{\"bauplaene\": 500}");
    let _ = fs::remove_dir_all(&root);
}
