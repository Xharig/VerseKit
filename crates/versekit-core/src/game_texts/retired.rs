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

//! What older versions wrote for the channels this build does not work on any more.
//!
//! The build players get knows no channel under the terms of confidentiality. On the first start it takes back what
//! earlier versions put there: the notes and the placed translation in the text files of the game folders, the
//! language line of the `user.cfg`, and the own data of those channels. The step is written down once; it is
//! not part of the test build, which works on those channels.

use std::fs;
use std::path::Path;

use crate::errors;
use crate::game_ini;
use crate::injection::{self, Outcome};
use crate::jsonio::{self, Value};
use crate::paths::{CHANNEL_DATA_FOLDER, Paths, SETTING_CHANNEL_FOLDERS};
use crate::translation;
use crate::writeguard;

/// The channels earlier versions worked on and this build does not know.
pub const RETIRED: [&str; 2] = ["EPTU", "TECH-PREVIEW"];

/// The setting that says the step was done (`kanaele_bereinigt`).
pub const DONE_SETTING: &str = "kanaele_bereinigt";

/// What the step took back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cleaned {
    /// The game folders of the channels that were looked at.
    pub folders: usize,
    /// The spots of notes taken out of text files.
    pub spots: usize,
    /// The text files and `user.cfg` files put back as they were before VerseKit.
    pub files: usize,
    /// The folders and files of the own data that were removed.
    pub data: usize,
}

impl Cleaned {
    /// Whether anything was taken back.
    pub fn any(&self) -> bool {
        self.spots + self.files + self.data > 0
    }
}

/// Takes back what earlier versions wrote for [`RETIRED`], once: `None` when that was done before, and on a copy of
/// the data, which never touches a game folder. The result is written into the log of notes when anything was taken
/// back.
pub fn run(paths: &Paths) -> Option<Cleaned> {
    if crate::testmode::on_copy(paths) || paths.setting_bool(DONE_SETTING, false) {
        return None;
    }
    let mut cleaned = Cleaned::default();
    for name in RETIRED {
        for root in paths.folders_of_channel(name) {
            cleaned.folders += 1;
            clean_game_folder(paths, &root, &mut cleaned);
        }
        clean_own_data(paths, name, &mut cleaned);
    }
    forget_choices(paths);
    paths.set_setting(DONE_SETTING, Value::Bool(true));
    if cleaned.any() {
        errors::note(
            "kanaele.bereinigt",
            &format!(
                "channels without support: {} game folders, {} spots of notes, {} files put back, {} own data removed",
                cleaned.folders, cleaned.spots, cleaned.files, cleaned.data
            ),
        );
    }
    Some(cleaned)
}

/// The notes out of the text files of a game folder, the text files and the `user.cfg` as they were before VerseKit
/// first changed them (the oldest copy kept beside them).
fn clean_game_folder(paths: &Paths, root: &str, cleaned: &mut Cleaned) {
    if translation::only_looks(paths, root) {
        return;
    }
    for (target, language) in super::files_with_notes(paths, root) {
        if let Outcome::Done(spots) = injection::remove_texts(paths, &target, &language) {
            cleaned.spots += spots;
        }
    }
    for (target, _) in super::language_files(paths, root) {
        if put_back_oldest_copy(paths, &target) {
            cleaned.files += 1;
        }
    }
    let user_cfg = paths.host.flavor.join(root, "user.cfg");
    let copy = paths.host.flavor.join(root, translation::USER_CFG_BACKUP);
    if game_ini::is_file(&copy)
        && fs::read(&copy).is_ok_and(|bytes| injection::write_game_file(&user_cfg, &bytes).is_ok())
    {
        if let Err(error) = jsonio::remove_file(&copy) {
            errors::record("kanaele.bereinigt", &error);
        }
        cleaned.files += 1;
    }
}

/// Writes the oldest copy of a text file back over it and removes the copies: the file as it was before VerseKit
/// replaced it the first time. Without a copy the file stays as it is.
fn put_back_oldest_copy(paths: &Paths, target: &str) -> bool {
    let copies = translation::backups_of(target);
    let Some(oldest) = copies.first() else { return false };
    let Ok(bytes) = fs::read(oldest) else { return false };
    if let Err(error) = injection::write_game_file(target, &bytes) {
        errors::record("kanaele.bereinigt", &error);
        return false;
    }
    injection::discard_origtext_for(paths, target);
    for copy in &copies {
        if let Err(error) = translation::remove_backup(copy) {
            errors::record("kanaele.bereinigt", &error);
        }
    }
    true
}

/// The own data of a channel: its folder below `Kanaele` and the note of the original texts of its file.
fn clean_own_data(paths: &Paths, name: &str, cleaned: &mut Cleaned) {
    let channels = paths.host.flavor.join(&paths.app_folder(), CHANNEL_DATA_FOLDER);
    if let Ok(entries) = fs::read_dir(&channels) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
                cleaned.data += remove_tree(&entry.path());
            }
        }
    }
    let note = paths.app_file(&format!("injektion-urtext@{}.json", name.to_lowercase()));
    if Path::new(&note).is_file() && jsonio::remove_file(&note).is_ok() {
        cleaned.data += 1;
    }
}

/// Removes a file or a folder with everything in it, each step checked by the write lock; how many files and folders
/// went.
fn remove_tree(path: &Path) -> usize {
    let mut count = 0;
    if path.is_dir() && !path.is_symlink() {
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            count += remove_tree(&entry.path());
        }
        if writeguard::check("rmdir", &path.to_string_lossy()).is_ok() && fs::remove_dir(path).is_ok() {
            count += 1;
        }
    } else if jsonio::remove_file(&path.to_string_lossy()).is_ok() {
        count += 1;
    }
    count
}

/// The folders and translations chosen for the retired channels are forgotten.
fn forget_choices(paths: &Paths) {
    if let Some(Value::Object(mut chosen)) = paths.settings_value(SETTING_CHANNEL_FOLDERS) {
        let before = chosen.len();
        chosen.retain(|(key, _)| !RETIRED.iter().any(|name| key.eq_ignore_ascii_case(name)));
        if chosen.len() != before {
            paths.set_setting(SETTING_CHANNEL_FOLDERS, Value::Object(chosen));
        }
    }
    for name in RETIRED {
        if translation::channel_sources(paths).iter().any(|(key, _)| key == name) {
            translation::set_channel_source(paths, name, "");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::game_texts::tests::Setup;
    use crate::game_texts::{NoArchive, prepare};

    const ORIGINAL: &str = "Bounty_title_001=Original\nBounty_desc_001=Wie vorher.\n";

    /// A game folder `name` beside LIVE as an earlier version left it: its text file replaced by a translation and
    /// holding the notes, the copy of the file from before beside it, the language line changed in the `user.cfg`.
    /// Returns the folder and its text file.
    fn left_by_an_older_version(s: &Setup, name: &str) -> (String, String) {
        let live = s.paths.game_folder().expect("the game folder");
        let root = format!("{}/{name}", Path::new(&live).parent().unwrap().to_string_lossy());
        let folder = format!("{root}/data/Localization/german_(germany)");
        fs::create_dir_all(&folder).unwrap();
        fs::write(format!("{root}/Data.p4k"), "").unwrap();
        let ini = format!("{folder}/global.ini");
        fs::write(&ini, ORIGINAL).unwrap();
        fs::write(format!("{root}/user.cfg"), "g_language = english\n").unwrap();
        translation::replace_game_file(&s.paths, &ini, b"Bounty_title_001=Kopfgeld\nBounty_desc_001=Fang ihn.\n", 3)
            .unwrap();
        assert!(translation::set_user_cfg(&s.paths, &root, "german_(germany)", None));
        let inputs = prepare(&s.paths, &NoArchive, "");
        assert!(matches!(
            injection::apply_texts(&s.paths, &ini, "german_(germany)", &inputs, false),
            Outcome::Done(_)
        ));
        assert!(injection::is_applied(&ini), "the notes stand in the file");
        (root, ini)
    }

    /// The own data of the retired channels: a folder each below `Kanaele`, a note of original texts each.
    fn own_data_of(s: &Setup) {
        for name in RETIRED {
            let folder = format!("{}/{CHANNEL_DATA_FOLDER}/{name}", s.paths.app_folder());
            fs::create_dir_all(&folder).unwrap();
            fs::write(format!("{folder}/einstellungen.json"), "{}").unwrap();
            fs::write(s.paths.app_file(&format!("injektion-urtext@{}.json", name.to_lowercase())), "{}").unwrap();
        }
        let ptu = format!("{}/{CHANNEL_DATA_FOLDER}/PTU", s.paths.app_folder());
        fs::create_dir_all(&ptu).unwrap();
        fs::write(format!("{ptu}/einstellungen.json"), "{}").unwrap();
    }

    #[test]
    fn what_earlier_versions_wrote_for_the_retired_channels_is_taken_back() {
        let s = Setup::new("kanal-aufraeumen");
        s.set_up_german();
        let (eptu, eptu_ini) = left_by_an_older_version(&s, "EPTU");
        let (tech, tech_ini) = left_by_an_older_version(&s, "TECH-PREVIEW");
        own_data_of(&s);
        translation::set_channel_source(&s.paths, "EPTU", "luftwerft_de");
        translation::set_channel_source(&s.paths, "PTU", "luftwerft_de");

        let cleaned = run(&s.paths).expect("the first start does it");
        assert!(cleaned.any() && cleaned.folders == 2, "{cleaned:?}");
        for (root, ini) in [(&eptu, &eptu_ini), (&tech, &tech_ini)] {
            assert_eq!(fs::read_to_string(ini).unwrap(), ORIGINAL, "the file is as before VerseKit: {root}");
            assert!(!injection::is_applied(ini), "no notes: {root}");
            assert!(translation::backups_of(ini).is_empty(), "the copies of VerseKit are gone: {root}");
            assert_eq!(fs::read_to_string(format!("{root}/user.cfg")).unwrap(), "g_language = english\n", "{root}");
            assert!(!Path::new(&format!("{root}/user.cfg.versekit-vorher")).exists(), "{root}");
        }
        let app = s.paths.app_folder();
        for name in RETIRED {
            assert!(!Path::new(&format!("{app}/{CHANNEL_DATA_FOLDER}/{name}")).exists(), "{name}");
            assert!(!Path::new(&s.paths.app_file(&format!("injektion-urtext@{}.json", name.to_lowercase()))).exists());
        }
        assert!(Path::new(&format!("{app}/{CHANNEL_DATA_FOLDER}/PTU/einstellungen.json")).exists(), "PTU stays");
        let chosen = translation::channel_sources(&s.paths);
        assert!(chosen.iter().all(|(k, _)| k != "EPTU") && chosen.iter().any(|(k, _)| k == "PTU"), "{chosen:?}");
        // The main installation is not touched.
        assert!(fs::read_to_string(s.ini()).unwrap().contains("Kopfgeld"));
    }

    #[test]
    fn the_step_is_written_down_and_done_once() {
        let s = Setup::new("kanal-aufraeumen-einmal");
        s.set_up_german();
        let (_, ini) = left_by_an_older_version(&s, "EPTU");
        let (first, noted) = errors::captured(|| run(&s.paths));
        assert!(first.is_some());
        assert!(
            noted.iter().any(|n| n.note && n.label == "kanaele.bereinigt"),
            "a line for the log of notes: {noted:?}"
        );
        // What stands there afterwards is not touched again.
        fs::write(&ini, "Bounty_title_001=Neu\n").unwrap();
        let (second, noted) = errors::captured(|| run(&s.paths));
        assert_eq!(second, None, "done before");
        assert!(noted.is_empty(), "{noted:?}");
        assert_eq!(fs::read_to_string(&ini).unwrap(), "Bounty_title_001=Neu\n");
    }

    #[test]
    fn a_start_without_anything_left_writes_no_line() {
        let s = Setup::new("kanal-aufraeumen-leer");
        s.set_up_german();
        let (cleaned, noted) = errors::captured(|| run(&s.paths));
        assert_eq!(cleaned, Some(Cleaned::default()));
        assert!(noted.is_empty(), "{noted:?}");
        assert_eq!(run(&s.paths), None, "and it is not looked for again");
    }

    #[test]
    fn a_file_without_a_copy_from_before_loses_its_notes_and_stays_otherwise() {
        let s = Setup::new("kanal-aufraeumen-ohne-kopie");
        s.set_up_german();
        let (_, ini) = left_by_an_older_version(&s, "EPTU");
        for copy in translation::backups_of(&ini) {
            fs::remove_file(&copy).unwrap();
            let _ = fs::remove_file(format!("{copy}.urtext.json"));
        }
        run(&s.paths).expect("done");
        let text = fs::read_to_string(&ini).unwrap();
        assert!(!injection::is_applied(&ini), "the notes are out: {text}");
        assert!(text.contains("Kopfgeld"), "the translation stays, there is no copy of before: {text}");
    }

    #[test]
    fn a_copy_of_the_data_is_left_alone() {
        let s = Setup::new("kanal-aufraeumen-kopie");
        s.set_up_german();
        let (_, ini) = left_by_an_older_version(&s, "EPTU");
        let before = fs::read_to_string(&ini).unwrap();
        let folder = std::env::temp_dir().join(format!("versekit-kopie-{}", crate::testdir::run_tag()));
        fs::create_dir_all(&folder).unwrap();
        let copy = Paths::for_copy(s.paths.host.clone(), &folder.to_string_lossy()).expect("a copy of the data");
        assert_eq!(run(&copy), None);
        let _ = fs::remove_dir_all(&folder);
        assert_eq!(fs::read_to_string(&ini).unwrap(), before, "a copy of the data never touches the game folders");
        assert!(injection::is_applied(&ini));
    }
}
