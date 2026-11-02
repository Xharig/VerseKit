// Verse-Kit — companion for Star Citizen.
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

//! The clocks of the background and a whole run, against a stand-in for the
//! net and the archive, in a throwaway game folder.

use std::fs;
use std::sync::Mutex;

use super::*;
use crate::net::Fake;
use crate::paths::Host;
use crate::pypath::Flavor;

pub(super) struct Setup {
    root: String,
    pub(super) paths: Paths,
    live: String,
}

impl Setup {
    pub(super) fn new(name: &str) -> Setup {
        let root = std::env::temp_dir().join(format!("versekit-gametexts-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let root = root.to_string_lossy().into_owned();
        let live = format!("{root}/StarCitizen/LIVE");
        let data = format!("{root}/data");
        fs::create_dir_all(format!("{live}/data/Localization/german_(germany)")).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(format!("{live}/Game.log"), "").unwrap();
        let host = Host::new(
            Flavor::Posix,
            &[("HOME", root.as_str()), ("SC_BP_HOME", &data), ("SC_INSTALL_DIR", &live)],
            &root,
        );
        Setup { root, paths: Paths::new(host), live }
    }

    pub(super) fn ini(&self) -> String {
        format!("{}/data/Localization/german_(germany)/global.ini", self.live)
    }

    /// The German translation as set up: file, note, choice, catalog.
    pub(super) fn set_up_german(&self) {
        fs::write(self.ini(), "Bounty_title_001=Kopfgeld\nBounty_desc_001=Fang ihn.\n").unwrap();
        translation::note_set(&self.paths, "deutsch", Some("git-1"), Some("german_(germany)"), None);
        self.paths.set_setting(game_ini::SETTING_SOURCE, Value::str("deutsch"));
        let catalog = r#"{"geholt": "2026-10-22 04:00", "bauplaene": {}, "missionen": {"Bounty_title_001": {"bp": ["Atzkav Sniper Rifle"], "rep": 0, "titel_key": "Bounty_title_001", "text_key": "Bounty_desc_001"}}}"#;
        fs::write(self.paths.app_file(sources::CATALOG_412), catalog).unwrap();
    }
}

impl Drop for Setup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// An archive whose stamp the test sets; it places a small English file,
/// or keeps a foreign one, and stores `names` as the original names.
#[derive(Default)]
pub(super) struct FakeArchive {
    stamp: Mutex<Option<String>>,
    gaps: usize,
    keep: bool,
    names: Option<&'static str>,
    names_refreshed: Mutex<usize>,
}

const ENGLISH: &str = "Bounty_title_001=Bounty\nBounty_desc_001=Catch him.\n";

impl Archive for FakeArchive {
    fn stamp(&self, _: &Paths) -> Option<String> {
        self.stamp.lock().unwrap().clone()
    }
    fn refresh_original(&self, _: &Paths) -> bool {
        false
    }
    fn fill_gaps(&self, _: &Paths, _: &str) -> usize {
        self.gaps
    }
    fn fetch_original(&self, paths: &Paths, root: &str, set_language: bool) -> Result<Placed, String> {
        if self.keep {
            return Ok(Placed { said: "kept".into(), written: false });
        }
        let folder = format!("{root}/data/Localization/english");
        fs::create_dir_all(&folder).unwrap();
        fs::write(format!("{folder}/global.ini"), ENGLISH).unwrap();
        if set_language {
            translation::set_user_cfg(paths, root, "english", None);
        }
        Ok(Placed { said: "1.0 MB".into(), written: true })
    }
    fn refresh_names(&self, paths: &Paths) {
        *self.names_refreshed.lock().unwrap() += 1;
        if let Some(names) = self.names {
            fs::write(paths.app_file(sources::NAMES_FILE), names).unwrap();
        }
    }
}

/// A file written in text mode with `text`, as the system leaves it: on
/// Windows every line break is `\r\n`.
fn native(text: &str) -> String {
    text.replace('\n', if cfg!(windows) { "\r\n" } else { "\n" })
}

fn english_ini(s: &Setup) -> String {
    format!("{}/data/Localization/english/global.ini", s.live)
}

#[test]
fn switching_to_the_original_resets_the_old_file_and_writes_the_new_one() {
    let s = Setup::new("switch");
    s.set_up_german();
    apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    assert!(injection::is_applied(&s.ini()));
    let done = switch_source(&s.paths, &Fake::default(), &FakeArchive::default(), ORIGINAL, "", &|_| {}).unwrap();
    assert_eq!(done.fetched, "1.0 MB");
    assert!(done.cleaned.is_some(), "the German file had notes");
    // The old file has its own wording back, the new one carries the notes.
    assert_eq!(fs::read_to_string(s.ini()).unwrap(), "Bounty_title_001=Kopfgeld\nBounty_desc_001=Fang ihn.\n");
    assert!(matches!(done.applied, Some(Outcome::Done(_))), "{:?}", done.applied);
    assert!(fs::read_to_string(english_ini(&s)).unwrap().contains("Bounty <EM4>[BP]</EM4>"));
    // The note of the original remembers it, so the background renews it.
    assert!(translation::installed(&s.paths, ORIGINAL, None).is_some());
    assert_eq!(s.paths.setting(game_ini::SETTING_SOURCE).as_deref(), Some(ORIGINAL));
}

#[test]
fn a_kept_foreign_file_is_no_original_and_switched_off_notes_stay_out() {
    let s = Setup::new("kept");
    s.paths.set_setting(SETTING_ON, Value::Bool(false));
    let archive = FakeArchive { keep: true, ..FakeArchive::default() };
    let done = switch_source(&s.paths, &Fake::default(), &archive, ORIGINAL, "", &|_| {}).unwrap();
    assert_eq!(done.fetched, "kept");
    assert_eq!(done.applied, None, "notes are switched off");
    assert!(translation::installed(&s.paths, ORIGINAL, None).is_none(), "a kept file is not noted as ours");
    // Placed by us, but notes off: the file stays without notes.
    let done = switch_source(&s.paths, &Fake::default(), &FakeArchive::default(), ORIGINAL, "", &|_| {}).unwrap();
    assert_eq!(done.applied, None);
    assert_eq!(fs::read_to_string(english_ini(&s)).unwrap(), ENGLISH);
}

#[test]
fn the_original_in_a_side_channel_leaves_the_main_note_alone() {
    let s = Setup::new("side");
    s.set_up_german();
    apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    let note = injection::load_origtext(&s.paths);
    let ptu = format!("{}/StarCitizen/PTU", s.root);
    fs::create_dir_all(&ptu).unwrap();
    let said = switch_side(&s.paths, &Fake::default(), &FakeArchive::default(), "PTU", &ptu, ORIGINAL, &|_| {});
    assert_eq!(said.as_deref(), Ok("1.0 MB"));
    assert!(fs::read_to_string(format!("{ptu}/data/Localization/english/global.ini")).unwrap() == ENGLISH);
    assert_eq!(injection::load_origtext(&s.paths), note, "the note of the main file is untouched");
    assert!(translation::installed(&s.paths, ORIGINAL, Some("PTU")).is_some());
    assert!(translation::installed(&s.paths, ORIGINAL, None).is_none());
    // The main file can still be taken back to its wording.
    remove(&s.paths);
    assert_eq!(fs::read_to_string(s.ini()).unwrap(), "Bounty_title_001=Kopfgeld\nBounty_desc_001=Fang ihn.\n");
}

#[test]
fn a_restored_copy_brings_its_original_texts_back() {
    let s = Setup::new("restore");
    s.set_up_german();
    apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    let with_notes = fs::read_to_string(s.ini()).unwrap();
    // A new translation replaces the file with notes, as a fetch does.
    let backup = translation::backup_game_file(&s.paths, &s.ini()).unwrap().unwrap();
    assert!(game_ini::is_file(&format!("{backup}.urtext.json")), "the note travels with the copy");
    fs::write(s.ini(), "Bounty_title_001=Kopfgeld zwei\nBounty_desc_001=Fang ihn jetzt.\n").unwrap();
    injection::discard_origtext_for(&s.paths, &s.ini());
    apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    // Compared as paths: the listing joins with the system's separator, the
    // copy was spelt with the test's.
    let listed: Vec<std::path::PathBuf> = translation::backups_of(&s.ini()).into_iter().map(Into::into).collect();
    assert_eq!(listed, vec![std::path::PathBuf::from(&backup)], "the note is no copy of its own");

    translation::restore_backup(&s.paths, &s.ini(), true).unwrap();
    assert_eq!(fs::read_to_string(s.ini()).unwrap(), with_notes);
    assert!(!injection::load_origtext(&s.paths).is_empty(), "the original texts of the copy are back");
    assert!(!game_ini::is_file(&format!("{backup}.urtext.json")), "used up with the copy");
    // So the notes of the copy come out exactly.
    remove(&s.paths);
    assert_eq!(fs::read_to_string(s.ini()).unwrap(), "Bounty_title_001=Kopfgeld\nBounty_desc_001=Fang ihn.\n");
}

#[test]
fn the_original_texts_of_another_file_never_go_into_this_one() {
    let s = Setup::new("otherfile");
    s.set_up_german();
    apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    // The English file comes beside it, the German one with its notes stays.
    let english = english_ini(&s);
    fs::create_dir_all(std::path::Path::new(&english).parent().unwrap()).unwrap();
    fs::write(&english, ENGLISH).unwrap();
    s.paths.set_setting(game_ini::SETTING_SOURCE, Value::str(ORIGINAL));
    apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    let text = fs::read_to_string(&english).unwrap();
    assert!(text.starts_with("Bounty_title_001=Bounty <EM4>[BP]</EM4>\n"), "{text}");
    assert!(!text.contains("Kopfgeld"), "{text}");
    // A copy put back keeps the note of a file that is still there.
    fs::write(&english, ENGLISH).unwrap();
    s.paths.set_setting(game_ini::SETTING_SOURCE, Value::str("deutsch"));
    apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    let note = injection::load_origtext(&s.paths);
    translation::backup_game_file(&s.paths, &english).unwrap();
    translation::restore_backup(&s.paths, &english, true).unwrap();
    assert_eq!(injection::load_origtext(&s.paths), note, "the German file still needs its note");
}

/// Without an English file beside it the ships the German file lacks come
/// from the original names: a run stores them from the archive first, so
/// the ships of the patch are in the file it writes.
#[test]
fn a_run_stores_the_original_names_before_it_writes() {
    let s = Setup::new("names");
    s.set_up_german();
    let archive =
        FakeArchive { names: Some(r#"{"vehicle_NameRSI_Ursa_Rover": "Ursa Rover"}"#), ..FakeArchive::default() };
    let outcome = apply(&s.paths, &Fake::default(), &archive, "");
    assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
    assert_eq!(*archive.names_refreshed.lock().unwrap(), 1);
    let text = fs::read_to_string(s.ini()).unwrap();
    assert!(text.ends_with("vehicle_NameRSI_Ursa_Rover=Ursa Rover\n"), "{text}");
}

/// A game folder given from outside is a test copy: an installation in the
/// usual places (here a Wine prefix in the home folder) is not found, and a
/// run that renews the translation of a PTU writes nothing there, though
/// only the real installation has a PTU.
#[test]
fn a_given_game_folder_keeps_the_real_installation_out() {
    let s = Setup::new("given");
    s.set_up_german();
    let real = format!("{}/.wine/drive_c/Program Files/Roberts Space Industries/StarCitizen", s.root);
    for channel in ["LIVE", "PTU"] {
        fs::create_dir_all(format!("{real}/{channel}/data/Localization/german_(germany)")).unwrap();
        fs::write(format!("{real}/{channel}/Data.p4k"), "").unwrap();
        fs::write(format!("{real}/{channel}/Game.log"), "").unwrap();
        fs::write(format!("{real}/{channel}/data/Localization/german_(germany)/global.ini"), "Bounty_title_001=Echt\n")
            .unwrap();
    }
    assert!(s.paths.game_roots().iter().any(|root| real.starts_with(root.as_str())), "the usual places hold it");

    let inside = format!("{}/StarCitizen/", s.root);
    let installed = s.paths.installed_channels();
    assert!(installed.iter().all(|(_, folder, _)| folder.starts_with(&inside)), "{installed:?}");
    assert_eq!(installed.iter().map(|(n, _, _)| n.as_str()).collect::<Vec<_>>(), ["LIVE"]);
    let available = s.paths.available_channels();
    assert!(available.iter().all(|(_, folder, _)| folder.starts_with(&inside)), "{available:?}");

    // The PTU has a translation of its own, and a newer one is out.
    let source = "luftwerft_de";
    translation::note_set(&s.paths, &translation::note_key(source, Some("PTU")), Some("etag:alt"), None, None);
    let url = "https://www.luftwerft.com/download/resources/localizations/PTU/german_(germany)/global.ini";
    // A file the size of a real one: a ship name and many lines.
    let file: String = ["Frontend_PU_Version=Star Citizen\n", "vehicle_NameAEGS_Gladius=Gladius\n"]
        .into_iter()
        .map(String::from)
        .chain((0..5001).map(|i| format!("Text_{i:05}=Zeile {i}\n")))
        .collect();
    let net = Fake::default().with_head(url, &[("etag", "\"neu\"")]).with(url, file);
    // A whole run with the lock on: every write stays in the copy and the data folder.
    crate::writeguard::deny(true);
    crate::writeguard::allow_under(Some(&inside));
    crate::writeguard::allow_also(&format!("{}/data", s.root));
    let job = Job {
        channels: vec![("PTU".into(), source.into())],
        source: Some("deutsch".into()),
        own_original: false,
        stock_only: false,
    };
    run(&job, &s.paths, &net, &FakeArchive::default(), "");
    let denied = crate::writeguard::take_denied();
    crate::writeguard::deny(false);
    assert!(denied.is_empty(), "{denied:?}");
    assert_eq!(
        fs::read_to_string(format!("{real}/PTU/data/Localization/german_(germany)/global.ini")).unwrap(),
        "Bounty_title_001=Echt\n"
    );
    assert!(fs::read_to_string(s.ini()).unwrap().contains("Kopfgeld <EM4>[BP]</EM4>"), "the copy got its notes");
}

/// A run on a copy of the data beside the real game: the lock lets nothing into the game folder, the translation
/// is only looked at and said calmly, and what the player reads names neither "being fetched" nor a refused step.
#[test]
fn on_a_copy_of_the_data_the_background_only_looks_and_no_inner_name_reaches_the_player() {
    let s = Setup::new("kopie-gesperrt");
    s.set_up_german();
    let source = "luftwerft_de";
    translation::note_set(&s.paths, source, Some("etag:alt"), Some("german_(germany)"), None);
    s.paths.set_setting(game_ini::SETTING_SOURCE, Value::str(source));
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/german_(germany)/global.ini";
    let file: String = ["Frontend_PU_Version=Star Citizen\n", "vehicle_NameAEGS_Gladius=Gladius\n"]
        .into_iter()
        .map(String::from)
        .chain((0..5001).map(|i| format!("Text_{i:05}=Zeile {i}\n")))
        .collect();
    let net = Fake::default().with_head(url, &[("etag", "\"neu\"")]).with(url, file);
    let before = fs::read_to_string(s.ini()).unwrap();
    crate::writeguard::deny(true);
    crate::writeguard::allow_under(Some(&format!("{}/data", s.root)));
    let job = Job { channels: Vec::new(), source: Some(source.into()), own_original: false, stock_only: false };
    run(&job, &s.paths, &net, &FakeArchive::default(), "");
    let applied = apply(&s.paths, &net, &FakeArchive::default(), "");
    let denied = crate::writeguard::take_denied();
    crate::writeguard::deny(false);
    crate::writeguard::allow_under(None);
    assert!(!denied.is_empty(), "the lock was asked");
    assert_eq!(fs::read_to_string(s.ini()).unwrap(), before, "the game file stays as it is");
    let said = match &applied {
        Outcome::WriteFailed(text) | Outcome::ReadFailed(text) => text.clone(),
        other => panic!("{other:?}"),
    };
    assert!(!net.asked().contains(&url.to_string()), "the file itself is not fetched: {:?}", net.asked());
    for lang in [crate::language::Lang::De, crate::language::Lang::En] {
        let state = translation::status_text(&s.paths, source, None, lang, &translation::today());
        let calm = if lang == crate::language::Lang::De {
            "im Testbetrieb nicht eingespielt"
        } else {
            "not placed in test mode"
        };
        assert!(state.contains(calm) && said.contains("VerseKit"), "{state} / {said}");
        assert!(!state.contains("gesperrt") && !state.contains("locked"), "{state}");
        for text in [&state, &said] {
            for inner in ["makedirs", "replace", "copy2", "Schreiben gesperrt", "wird geholt", "being fetched"] {
                assert!(!text.contains(inner), "{inner} in {text}");
            }
        }
    }
}

#[test]
fn the_active_source_is_the_choice_as_it_is() {
    let s = Setup::new("active");
    assert_eq!(active_source(&s.paths), None, "nothing set up");
    s.set_up_german();
    assert_eq!(active_source(&s.paths).as_deref(), Some("deutsch"));
    // A chosen source that is not set up is no source, even when another is.
    s.paths.set_setting(game_ini::SETTING_SOURCE, Value::str("starstrings"));
    assert_eq!(active_source(&s.paths), None);
    // „Leave alone“ and the original texts are choices of their own.
    s.paths.set_setting(game_ini::SETTING_SOURCE, Value::str(""));
    assert_eq!(active_source(&s.paths), None);
    s.paths.set_setting(game_ini::SETTING_SOURCE, Value::str(ORIGINAL));
    assert_eq!(active_source(&s.paths), None);
    // Without any choice the first set up counts.
    s.paths.set_setting(game_ini::SETTING_SOURCE, Value::Null);
    assert_eq!(active_source(&s.paths).as_deref(), Some("deutsch"));
}

#[test]
fn nothing_set_up_means_hands_off() {
    let s = Setup::new("handsoff");
    let mut upkeep = Upkeep::default();
    assert_eq!(upkeep.tick(&s.paths, &FakeArchive::default(), &BTreeSet::new(), 100.0), None);
}

#[test]
fn switched_off_pushes_the_clock_and_does_nothing() {
    let s = Setup::new("off");
    s.set_up_german();
    s.paths.set_setting(SETTING_AUTO, Value::Bool(false));
    let mut upkeep = Upkeep::default();
    assert_eq!(upkeep.tick(&s.paths, &FakeArchive::default(), &BTreeSet::new(), 100.0), None);
    assert_eq!(upkeep.texts_next, 100.0 + TEXTS_POLL_SECONDS);
}

/// `SC_BP_NO_NET` counts as the program was given it, not as the shell of the check has it:
/// set on the made-up computer nothing runs, unset the run is due.
#[test]
fn no_net_of_the_given_environment_stops_the_run() {
    let s = Setup::new("no-net");
    s.set_up_german();
    let mut off = s.paths.host.clone();
    off.set_var("SC_BP_NO_NET", "1");
    let offline = Paths::new(off);
    let archive = FakeArchive::default();
    assert_eq!(Upkeep::default().tick(&offline, &archive, &BTreeSet::new(), 1000.0), None);
    assert!(Upkeep::default().tick(&s.paths, &archive, &BTreeSet::new(), 1000.0).is_some(), "without it, due");
}

#[test]
fn start_hour_collection_file_and_game_end() {
    let s = Setup::new("clocks");
    s.set_up_german();
    let archive = FakeArchive::default();
    let owned = BTreeSet::new();
    let mut upkeep = Upkeep::default();
    let job = upkeep.tick(&s.paths, &archive, &owned, 1000.0).expect("due at start");
    assert_eq!(job.source.as_deref(), Some("deutsch"));
    assert!(!job.stock_only);
    assert_eq!(upkeep.tick(&s.paths, &archive, &owned, 1001.0), None, "no second run while one goes on");
    upkeep.finished(&s.paths, false);
    assert_eq!(upkeep.texts_next, 1000.0 + TEXTS_POLL_SECONDS, "every five minutes");
    assert_eq!(upkeep.texts_next, 1300.0);

    // The collection changed: only the boxes, no look at foreign changes.
    s.paths.set_setting(SETTING_STOCK_MARK, Value::str("old"));
    let job = upkeep.tick(&s.paths, &archive, &owned, 1040.0).expect("collection changed");
    assert!(job.stock_only);
    upkeep.finished(&s.paths, false);
    s.paths.set_setting(SETTING_STOCK_MARK, Value::str(stock_mark(&s.paths, &owned)));

    // Another tool rewrites the file.
    assert_eq!(upkeep.tick(&s.paths, &archive, &owned, 1080.0), None);
    fs::write(s.ini(), "Bounty_title_001=Kopfgeld neu geschrieben\n").unwrap();
    let job = upkeep.tick(&s.paths, &archive, &owned, 1090.0).expect("file rewritten");
    assert!(!job.stock_only);
    upkeep.finished(&s.paths, false);
    assert_eq!(upkeep.tick(&s.paths, &archive, &owned, 1100.0), None, "the own writing is no foreign one");

    upkeep.game_ended();
    assert!(upkeep.tick(&s.paths, &archive, &owned, 1110.0).is_some(), "after a game session at once");
}

#[test]
fn a_finished_patch_counts_like_the_normal_poll_and_then_every_five_minutes() {
    let s = Setup::new("patch");
    s.set_up_german();
    let archive = FakeArchive::default();
    *archive.stamp.lock().unwrap() = Some("1:1".into());
    let owned = BTreeSet::new();
    let mut upkeep = Upkeep::default();
    upkeep.tick(&s.paths, &archive, &owned, 0.0);
    upkeep.finished(&s.paths, false);
    s.paths.set_setting(SETTING_STOCK_MARK, Value::str(stock_mark(&s.paths, &owned)));
    // First look remembers the stamp.
    assert_eq!(upkeep.tick(&s.paths, &archive, &owned, 10.0), None);
    assert_eq!(s.paths.setting(P4K_SETTING).as_deref(), Some("1:1"));
    *archive.stamp.lock().unwrap() = Some("2:2".into());
    assert_eq!(upkeep.tick(&s.paths, &archive, &owned, 20.0), None, "patch running");
    assert_eq!(upkeep.tick(&s.paths, &archive, &owned, 40.0), None, "still quiet time");
    let job = upkeep.tick(&s.paths, &archive, &owned, 60.0).expect("patch finished");
    assert!(!job.stock_only);
    assert_eq!(upkeep.texts_next, 60.0 + POST_PATCH_POLL_SECONDS);
    assert_eq!(upkeep.texts_next, 360.0, "five minutes after the patch");
    upkeep.finished(&s.paths, true);
    assert_eq!(upkeep.patch_at, 0.0, "the new translation came: back to the normal poll");
}

#[test]
fn the_translation_is_looked_at_every_five_minutes_in_the_normal_poll_and_after_a_patch() {
    assert_eq!((TEXTS_POLL_SECONDS, POST_PATCH_POLL_SECONDS), (300.0, 300.0));
    let mut upkeep = Upkeep::default();
    assert_eq!(upkeep.texts_interval(0.0), 300.0, "no patch seen");
    upkeep.patch_at = 100.0;
    assert_eq!(upkeep.texts_interval(200.0), 300.0, "inside the window after a patch");
    assert_eq!(upkeep.texts_interval(100.0 + POST_PATCH_WINDOW_SECONDS + 1.0), 300.0, "after the window");
}

#[test]
fn a_run_writes_the_notes_repairs_the_language_and_remembers_the_mark() {
    let s = Setup::new("run");
    s.set_up_german();
    let archive = FakeArchive { gaps: 0, ..FakeArchive::default() };
    let net = Fake::default();
    let job = Job { channels: Vec::new(), source: Some("deutsch".into()), own_original: false, stock_only: false };
    let ran = run(&job, &s.paths, &net, &archive, "");
    assert!(ran.notices.contains(&Notice::LanguageRepaired("german_(germany)".into())), "{:?}", ran.notices);
    assert!(ran.notices.iter().any(|n| matches!(n, Notice::Applied(_))), "{:?}", ran.notices);
    assert!(fs::read_to_string(s.ini()).unwrap().contains("Kopfgeld <EM4>[BP]</EM4>"));
    assert_eq!(
        fs::read_to_string(format!("{}/user.cfg", s.live)).unwrap(),
        native("g_language = german_(germany)\ng_languageAudio = english\n")
    );
    assert_eq!(s.paths.setting(SETTING_CATALOG).as_deref(), Some("2026-10-22 04:00"));
    assert!(s.paths.setting(SETTING_STOCK_MARK).is_some());
    // A second run finds everything in place and writes nothing.
    let again = run(&job, &s.paths, &net, &archive, "");
    assert!(again.notices.is_empty(), "{:?}", again.notices);
}

#[test]
fn on_a_copy_of_the_data_the_translation_is_only_looked_at_by_the_page_and_the_run() {
    let s = Setup::new("testkopie");
    s.set_up_german();
    s.paths.set_given_data(&format!("{}/data", s.root), crate::given::GivenBy::CommandLine);
    let spec = translation::spec(&s.paths, "deutsch", None).expect("source");
    let (repo, file) = (spec.repo.unwrap_or_default(), spec.repo_file.unwrap_or_default());
    let commits = format!("https://api.github.com/repos/{repo}/commits?path={file}&per_page=1");
    let raw = format!("https://raw.githubusercontent.com/{repo}/abcdef1234567890/{file}");
    let net = Fake::default()
        .with(&commits, r#"[{"sha": "abcdef1234567890", "commit": {"committer": {"date": "2026-10-28T09:00:00Z"}}}]"#)
        .with(&raw, "Bounty_title_001=Neu\nvehicle_x=y\n");
    // The button of the page: a calm sentence, no failure, nothing fetched or written into the game.
    crate::language::set_language(crate::language::Choice::De);
    let done = switch_source(&s.paths, &net, &FakeArchive::default(), "deutsch", "", &|_| {}).expect("no failure");
    assert_eq!(done.fetched, "Neue Fassung vom 28.10.2026 gibt es, im Testbetrieb nicht eingespielt.");
    assert!(done.cleaned.is_none() && done.applied.is_none());
    // The run in the background: the same look, no download either.
    let job = Job { channels: Vec::new(), source: Some("deutsch".into()), own_original: false, stock_only: false };
    let ran = run(&job, &s.paths, &net, &FakeArchive::default(), "");
    assert!(!ran.renewed && !ran.notices.iter().any(|n| matches!(n, Notice::TranslationUpdated(_))));
    assert!(!net.asked().contains(&raw), "the file is never fetched: {:?}", net.asked());
    let text = fs::read_to_string(s.ini()).unwrap();
    assert!(text.contains("Kopfgeld") && !text.contains("=Neu"), "the translation is not replaced: {text}");
    assert_eq!(translation::installed(&s.paths, "deutsch", None).as_deref(), Some("git-1"), "nothing noted as placed");
    let state = translation::status_text(&s.paths, "deutsch", None, crate::language::Lang::De, &translation::today());
    assert!(state.contains("im Testbetrieb nicht eingespielt") && !state.contains("gesperrt"), "{state}");
}

#[test]
fn removing_and_status() {
    let s = Setup::new("status");
    s.set_up_german();
    assert!(!status(&s.paths).applied);
    let outcome = apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    assert!(matches!(outcome, Outcome::Done(_)), "{outcome:?}");
    let now = status(&s.paths);
    assert!(now.applied);
    assert_eq!(now.source.as_deref(), Some("deutsch"));
    remove(&s.paths);
    assert!(!status(&s.paths).applied);
    assert_eq!(fs::read_to_string(s.ini()).unwrap(), "Bounty_title_001=Kopfgeld\nBounty_desc_001=Fang ihn.\n");
}

#[test]
fn a_source_is_confirmed_once() {
    let s = Setup::new("confirm");
    assert!(!confirmed(&s.paths, "deutsch"));
    confirm(&s.paths, "deutsch");
    confirm(&s.paths, "starstrings");
    confirm(&s.paths, "deutsch");
    assert_eq!(s.paths.setting(SETTING_CONFIRMED).as_deref(), Some("deutsch,starstrings"));
}

#[test]
fn the_report_line_tells_state_switches_source_and_language() {
    use crate::language::Lang;
    use crate::report_facts::injection_line;
    let s = Setup::new("report");
    assert_eq!(injection_line(&s.paths, Lang::De), "keine gefunden", "no file is no warning");
    s.set_up_german();
    let before = injection_line(&s.paths, Lang::De);
    assert!(before.starts_with("NICHT eingetragen · Auffrischen automatisch · deutsch git-1 "), "{before}");
    assert!(before.ends_with(" · german_(germany) / Spiel: —"), "{before}");
    apply(&s.paths, &Fake::default(), &FakeArchive::default(), "");
    s.paths.set_setting(SETTING_ON, Value::Bool(false));
    let after = injection_line(&s.paths, Lang::En);
    assert!(after.starts_with("in place · writing them is switched off · refreshes automatically"), "{after}");
}
