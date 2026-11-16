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
//! set below it. The real data are never reached.

use std::fs;

use super::*;
use crate::paths::{Host, SETTINGS_FILE};
use crate::pypath::Flavor;

fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("versekit-versionswechsel-{name}-{}", crate::testdir::run_tag()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().into_owned()
}

fn machine(root: &str) -> Paths {
    let f = Flavor::native();
    let home = f.join(root, "home");
    let config = f.join(root, "appdata");
    fs::create_dir_all(f.join(&home, "Dokumente")).unwrap();
    fs::create_dir_all(&config).unwrap();
    let vars = [
        ("USERPROFILE", home.as_str()),
        ("HOME", home.as_str()),
        ("APPDATA", config.as_str()),
        ("XDG_CONFIG_HOME", config.as_str()),
    ];
    Paths::new(Host::new(f, &vars, root))
}

fn put(path: &str, text: &str) {
    fs::create_dir_all(Flavor::native().dirname(path)).unwrap();
    fs::write(path, text).unwrap();
}

/// The data of a player: settings that say which version ran last (`last`, none when empty) and a collection.
fn data(p: &Paths, last: &str) {
    let f = Flavor::native();
    let folder = p.default_folder();
    let note = if last.is_empty() { String::new() } else { format!(", \"letzte_version\": \"{last}\"") };
    put(&f.join_all(&folder, &["Einstellungen", SETTINGS_FILE]), &format!("{{\"sprache\": \"de\"{note}}}"));
    put(&f.join_all(&folder, &["Bauplaene", "bestand.json"]), "{\"version\": 3, \"bauplaene\": {}}");
}

fn backups(p: &Paths) -> Vec<String> {
    let folder = Flavor::native().join_all(&p.app_folder(), &["Intern", backup::BEFORE_VERSION_CHANGE_FOLDER]);
    let mut names: Vec<String> = fs::read_dir(folder)
        .map(|entries| entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// A change of version is backed up before anything else, the version is noted afterwards, and the same version
/// at the next start makes no second backup.
#[test]
fn a_change_of_version_backs_up_first_and_notes_the_version() {
    let root = scratch("wechsel");
    let p = machine(&root);
    data(&p, "5.3.5");

    let Change::BackedUp { previous, file } = back_up_on_change(&p, "5.4.0") else { panic!("a change is backed up") };
    assert_eq!(previous.as_deref(), Some("5.3.5"));
    assert!(backup::check(&file).is_some_and(|found| found.count > 0), "the file is a backup that reads");
    assert!(file.contains(backup::BEFORE_VERSION_CHANGE_FOLDER) && file.ends_with("-5.4.0.zip"), "{file}");
    assert_eq!(last_version(&p).as_deref(), Some("5.4.0"));
    assert_eq!(backups(&p).len(), 1);

    assert_eq!(back_up_on_change(&p, "5.4.0"), Change::Same);
    assert_eq!(back_up_on_change(&p, "v5.4.0"), Change::Same, "the way a tag writes it is the same version");
    assert_eq!(backups(&p).len(), 1, "no second backup for the same version");
    let _ = fs::remove_dir_all(&root);
}

/// The way back to an older version is a change as well: it may read what the newer one wrote differently.
#[test]
fn the_way_back_to_an_older_version_is_a_change_too() {
    let root = scratch("zurueck");
    let p = machine(&root);
    data(&p, "5.4.0");
    let Change::BackedUp { previous, .. } = back_up_on_change(&p, "5.3.5") else { panic!("backed up") };
    assert_eq!(previous.as_deref(), Some("5.4.0"));
    assert_eq!(last_version(&p).as_deref(), Some("5.3.5"));
    let _ = fs::remove_dir_all(&root);
}

/// A version that was never noted (data of a version before this one) counts as a change.
#[test]
fn a_version_that_was_never_noted_counts_as_a_change() {
    let root = scratch("ungemerkt");
    let p = machine(&root);
    data(&p, "");
    let Change::BackedUp { previous, .. } = back_up_on_change(&p, "5.4.0") else { panic!("backed up") };
    assert_eq!(previous, None);
    assert_eq!(last_version(&p).as_deref(), Some("5.4.0"));
    let _ = fs::remove_dir_all(&root);
}

/// Without data (a first start) nothing is made and nothing is written: no settings file that the assistant of a
/// first start would take for a set-up program.
#[test]
fn without_data_nothing_is_made_and_nothing_is_written() {
    let root = scratch("leer");
    let p = machine(&root);
    assert_eq!(back_up_on_change(&p, "5.4.0"), Change::NoData);
    let settings = Flavor::native().join_all(&p.app_folder(), &["Einstellungen", SETTINGS_FILE]);
    assert!(fs::metadata(&settings).is_err(), "no settings file made");
    assert!(backups(&p).is_empty());
    let _ = fs::remove_dir_all(&root);
}

/// A backup that cannot be made keeps the old version in the note, says so, and holds the start back from
/// renaming and moving; the next start tries again.
#[test]
fn a_failed_backup_keeps_the_old_note_and_holds_the_start_back() {
    let root = scratch("scheitert");
    let p = machine(&root);
    data(&p, "5.3.5");
    // A file where the folder of the backups belongs.
    put(&Flavor::native().join_all(&p.app_folder(), &["Intern", backup::BEFORE_VERSION_CHANGE_FOLDER]), "im Weg");

    let result = back_up_on_change(&p, "5.4.0");
    assert!(matches!(result, Change::Failed(_)), "{result:?}");
    assert!(!result.may_go_on());
    assert_eq!(last_version(&p).as_deref(), Some("5.3.5"), "the note stays: the next start tries again");

    fs::remove_file(Flavor::native().join_all(&p.app_folder(), &["Intern", backup::BEFORE_VERSION_CHANGE_FOLDER]))
        .unwrap();
    assert!(back_up_on_change(&p, "5.4.0").may_go_on());
    assert_eq!(last_version(&p).as_deref(), Some("5.4.0"));
    let _ = fs::remove_dir_all(&root);
}

/// The backup is made in the folder under its earlier name, before the rename: it holds the files as they were
/// and goes along with the folder.
#[test]
fn the_backup_is_made_before_the_folder_under_the_earlier_name_is_renamed() {
    let root = scratch("umbenennen");
    let p = machine(&root);
    let f = Flavor::native();
    let old = f.join(&p.documents(), crate::legacy::DOCUMENTS_FOLDER);
    put(&f.join_all(&old, &["Einstellungen", SETTINGS_FILE]), "{\"sprache\": \"de\", \"letzte_version\": \"5.3.5\"}");
    put(&f.join_all(&old, &["Bauplaene", "bestand.json"]), "{\"version\": 3, \"bauplaene\": {}}");
    assert_eq!(p.default_folder(), old);

    let Change::BackedUp { file, .. } = back_up_on_change(&p, "5.4.0") else { panic!("backed up") };
    assert!(file.starts_with(&old), "made in the folder that holds the data: {file}");
    assert!(backup::check(&file).is_some_and(|found| found.count >= 2), "it holds the files as they were");
    assert!(matches!(p.rename_to_current_name(), crate::folder_rename::Renamed::Done { .. }));
    let moved = f.join_all(&p.default_folder(), &["Intern", backup::BEFORE_VERSION_CHANGE_FOLDER]);
    assert!(fs::read_dir(moved).is_ok_and(|entries| entries.count() == 1), "the backup went along with the folder");
    let _ = fs::remove_dir_all(&root);
}

/// A data folder that is not there (the pointer names a place that is gone) has nothing to keep: no failure.
#[test]
fn a_data_folder_that_is_gone_is_no_failure() {
    let root = scratch("fehlt");
    let p = machine(&root);
    let f = Flavor::native();
    let (second, _) = p.second_pointer_places();
    put(&second, &format!("{{\"ablage_ordner\": \"{}\"}}", f.join(&root, "Weg").replace('\\', "\\\\")));
    assert_eq!(back_up_on_change(&p, "5.4.0"), Change::NoData);
    let _ = fs::remove_dir_all(&root);
}
