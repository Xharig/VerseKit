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

//! The page a group head opens and the start page, as the settings keep them.

use super::*;
use crate::paths::Host;
use crate::pypath::Flavor;

fn paths(name: &str, settings: &str) -> (Paths, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("versekit-navigation-{name}-{}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let data = root.to_string_lossy().into_owned();
    if !settings.is_empty() {
        std::fs::write(root.join("einstellungen.json"), settings).unwrap();
    }
    let host = Host::new(Flavor::Posix, &[("HOME", data.as_str()), ("VERSEKIT_HOME", data.as_str())], &data);
    (Paths::new(host), root)
}

const PAGES: [&str; 3] = ["hf_liste", "hf_contested", "hf_auftragslog"];

#[test]
fn a_group_nobody_opened_a_page_of_opens_its_first_page() {
    let visits = Visits::default();
    assert_eq!(visits.target("bauplaene", &PAGES), Some("hf_liste"));
    assert_eq!(visits.target("bauplaene", &[]), None);
}

#[test]
fn the_page_opened_last_wins_over_the_one_opened_most() {
    let mut visits = Visits::default();
    for _ in 0..5 {
        visits.note("bauplaene", "hf_liste");
    }
    visits.note("bauplaene", "hf_auftragslog");
    assert_eq!(visits.target("bauplaene", &PAGES), Some("hf_auftragslog"));
}

#[test]
fn without_the_last_page_the_one_opened_most_is_taken_and_equals_go_to_the_first() {
    let mut visits = Visits::default();
    visits.note("bauplaene", "hf_contested");
    visits.note("bauplaene", "hf_contested");
    visits.note("bauplaene", "hf_auftragslog");
    visits.note("bauplaene", "hf_liste");
    // The last page (`hf_liste`) is not shown now, e.g. its module is off.
    let shown = ["hf_contested", "hf_auftragslog"];
    assert_eq!(visits.target("bauplaene", &shown), Some("hf_contested"));
    // Two pages equally often: the first of them in the menu.
    let mut equal = Visits::default();
    equal.note("bauplaene", "hf_auftragslog");
    equal.note("bauplaene", "hf_contested");
    equal.note("bauplaene", "hf_liste");
    assert_eq!(equal.target("bauplaene", &["hf_contested", "hf_auftragslog"]), Some("hf_contested"));
}

#[test]
fn a_page_that_is_gone_from_the_group_is_never_the_target() {
    let mut visits = Visits::default();
    visits.note("bauplaene", "seite_ohne_eintrag");
    assert_eq!(visits.target("bauplaene", &PAGES), Some("hf_liste"));
    // Another group's last page does not count either.
    visits.note("logistik", "hf_contested");
    assert_eq!(visits.target("bauplaene", &PAGES), Some("hf_liste"));
}

#[test]
fn what_was_opened_is_written_and_read_back() {
    let (paths, root) = paths("roundtrip", "");
    let mut visits = Visits::default();
    visits.note("bauplaene", "hf_contested");
    visits.note("bauplaene", "hf_contested");
    visits.note("logistik", "hf_hangar");
    for (key, value) in visits.values() {
        assert!(paths.set_setting(key, value));
    }
    let file = std::fs::read_to_string(root.join("einstellungen.json")).unwrap();
    assert!(file.contains("\"bereich_zuletzt\"") && file.contains("\"hf_contested\": 2"), "{file}");
    assert_eq!(Visits::load(&paths), visits);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_damaged_setting_is_read_as_nothing_opened() {
    let broken = concat!(
        r#"{"bereich_zuletzt": ["x"], "#,
        r#""bereich_zaehler": {"bauplaene": {"hf_liste": "viel", "hf_contested": -3}, "logistik": 7}}"#
    );
    let (paths, root) = paths("broken", broken);
    let visits = Visits::load(&paths);
    assert_eq!(visits.target("bauplaene", &PAGES), Some("hf_liste"));
    assert_eq!(visits.target("logistik", &["hf_hangar"]), Some("hf_hangar"));
    // What was read is no negative count either, not only what the group head does with it.
    assert!(visits.counts.iter().flat_map(|(_, pages)| pages).all(|(_, n)| *n >= 0), "{:?}", visits.counts);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_page_that_became_a_view_leads_to_the_page_it_went_into_and_counts_with_it() {
    let old = concat!(
        r#"{"bereich_zuletzt": {"bauplaene": "hf_fortschritt", "herstellung": "hf_farmliste", "bergbau": "hf_raffinerien"}, "#,
        r#""bereich_zaehler": {"bauplaene": {"hf_liste": 2, "hf_fortschritt": 5}, "herstellung": {"hf_farmliste": 3}, "#,
        r#""bergbau": {"hf_bergbau": 1, "hf_raffinerien": 4}}}"#
    );
    let (paths, root) = paths("moved", old);
    let visits = Visits::load(&paths);
    assert_eq!(visits.target("bauplaene", &["hf_liste", "hf_auftragslog"]), Some("hf_liste"));
    assert_eq!(visits.target("herstellung", &["hf_herstellung"]), Some("hf_herstellung"));
    let counts = |group: &str| visits.counts.iter().find(|(g, _)| g == group).map(|(_, pages)| pages.clone());
    assert_eq!(counts("bauplaene"), Some(vec![("hf_liste".to_string(), 7)]));
    assert_eq!(counts("herstellung"), Some(vec![("hf_herstellung".to_string(), 3)]));
    assert_eq!(visits.target("bergbau", &["hf_bergbau", "hf_bergung"]), Some("hf_bergbau"));
    assert_eq!(counts("bergbau"), Some(vec![("hf_bergbau".to_string(), 5)]));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_start_page_is_the_overview_unless_the_settings_name_one() {
    let (none, root) = paths("start-none", "");
    assert_eq!(start_page(&none), "");
    let (set, root2) = paths("start-set", "{\"startseite\": \" hf_contested \"}");
    assert_eq!(start_page(&set), "hf_contested");
    let (odd, root3) = paths("start-odd", "{\"startseite\": 5}");
    assert_eq!(start_page(&odd), "");
    for dir in [root, root2, root3] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
