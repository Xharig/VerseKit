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

use super::*;
use crate::net::Fake;
use crate::paths::Host;
use crate::pypath::Flavor;

fn paths(name: &str) -> (crate::testdir::Dir, Paths) {
    let dir = crate::testdir::Dir::new(&format!("luftwerft-list-{name}"));
    let root = dir.path().to_string_lossy().into_owned();
    let data = format!("{root}/data");
    std::fs::create_dir_all(&data).unwrap();
    let host = Host::new(Flavor::Posix, &[("HOME", root.as_str()), ("VERSEKIT_HOME", &data)], &root);
    (dir, Paths::new(host))
}

fn address(channel: &str, folder: &str) -> String {
    format!("{BASE}{channel}/{folder}/global.ini")
}

/// A list of the server with these folders in both channels.
pub(crate) fn list_of(folders: &[&str]) -> String {
    let part = |channel: &str| {
        folders
            .iter()
            .map(|folder| format!(r#""{folder}": {{"url": "{}"}}"#, address(channel, folder)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(r#"{{"LIVE": {{{}}}, "PTU": {{{}}}}}"#, part("LIVE"), part("PTU"))
}

const FIXED: [&str; 9] = [
    "english",
    "german_(germany)",
    "french_(france)",
    "italian_(italy)",
    "spanish_(spain)",
    "bavarian_(germany)",
    "dutch_(netherlands)",
    "portuguese_(portugal)",
    "portuguese_(brazil)",
];

#[test]
fn only_addresses_under_the_base_are_taken() {
    let text = format!(
        r#"{{"LIVE": {{
            "swedish_(sweden)": {{"url": "{good}"}},
            "evil_(one)": {{"url": "https://example.com/download/resources/localizations/LIVE/evil_(one)/global.ini"}},
            "plain_(two)": {{"url": "http://www.luftwerft.com/download/resources/localizations/LIVE/plain_(two)/global.ini"}},
            "up_(three)": {{"url": "{BASE}LIVE/../../x/global.ini"}},
            "query_(four)": {{"url": "{BASE}LIVE/query_(four)/global.ini?x=1"}},
            "Bad Folder": {{"url": "{BASE}LIVE/Bad Folder/global.ini"}},
            "none_(five)": {{}}
        }}}}"#,
        good = address("LIVE", "swedish_(sweden)")
    );
    let entries = parse(text.as_bytes()).expect("one good entry");
    assert_eq!(
        entries,
        [Entry { folder: "swedish_(sweden)".into(), live: Some(address("LIVE", "swedish_(sweden)")), ptu: None }]
    );
}

#[test]
fn no_list_is_no_list() {
    assert_eq!(parse(b"not json"), None);
    assert_eq!(parse(b"[]"), None);
    assert_eq!(parse(b"{}"), None);
    assert_eq!(parse(br#"{"LIVE": {}, "PTU": {}}"#), None);
}

#[test]
fn a_new_list_is_kept_and_a_broken_one_changes_nothing() {
    let (_dir, paths) = paths("keep");
    assert_eq!(load(&paths), None);
    assert!(extras(&paths).is_empty(), "without a list only the fixed sources stand");
    let mut folders = FIXED.to_vec();
    folders.push("swedish_(sweden)");
    let net = Fake::default().with(LIST_URL, list_of(&folders)).with_etag(LIST_URL, "\"a\"");
    assert_eq!(refresh(&net, &paths), Refresh::Changed);
    let kept = extras(&paths);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].folder, "swedish_(sweden)");
    assert_eq!(kept[0].ptu, Some(address("PTU", "swedish_(sweden)")));
    let broken = Fake::default().with(LIST_URL, "this is no list");
    assert_eq!(refresh(&broken, &paths), Refresh::Failed);
    assert_eq!(extras(&paths), kept, "a broken list changes nothing");
    let offline = Fake::default();
    assert_eq!(refresh(&offline, &paths), Refresh::Failed);
    assert_eq!(extras(&paths), kept, "nor does a missing one");
}

#[test]
fn the_second_look_asks_with_the_tag_and_survives_a_restart() {
    let (_dir, paths) = paths("tag");
    let net = Fake::default().with(LIST_URL, list_of(&FIXED)).with_etag(LIST_URL, "\"a\"");
    assert_eq!(refresh(&net, &paths), Refresh::Changed);
    assert_eq!(refresh(&net, &paths), Refresh::Unchanged);
    assert_eq!(net.asked(), [LIST_URL.to_string(), format!("{LIST_URL} (unchanged)")]);
    assert!(std::path::Path::new(&paths.app_file(FILE)).is_file());
    // A new program run reads the file again, not its memory.
    MEMO.lock().unwrap().clear();
    let again = Fake::default().with(LIST_URL, list_of(&FIXED)).with_etag(LIST_URL, "\"a\"");
    assert_eq!(refresh(&again, &paths), Refresh::Unchanged);
    assert_eq!(again.asked(), [format!("{LIST_URL} (unchanged)")]);
}

#[test]
fn a_language_that_leaves_the_list_is_remembered_and_a_fixed_one_is_not_added_twice() {
    let (_dir, paths) = paths("gone");
    let mut folders = FIXED.to_vec();
    folders.push("swedish_(sweden)");
    let first = Fake::default().with(LIST_URL, list_of(&folders));
    assert_eq!(refresh(&first, &paths), Refresh::Changed);
    assert_eq!(lists(&paths, "swedish_(sweden)", false), Some(true));
    assert_eq!(lists(&paths, "swedish_(sweden)", true), Some(true));
    let fewer: Vec<&str> = FIXED.iter().copied().filter(|f| *f != "dutch_(netherlands)").collect();
    let second = Fake::default().with(LIST_URL, list_of(&fewer));
    assert_eq!(refresh(&second, &paths), Refresh::Changed);
    let kept = extras(&paths);
    assert_eq!(kept.iter().map(|e| e.folder.as_str()).collect::<Vec<_>>(), ["swedish_(sweden)"]);
    assert_eq!(lists(&paths, "swedish_(sweden)", false), Some(false), "it is gone from the list");
    assert_eq!(lists(&paths, "dutch_(netherlands)", false), Some(false), "a fixed one too, but it is no extra");
    assert_eq!(lists(&paths, "english", false), Some(true));
}

#[test]
fn a_folder_with_only_one_channel_is_only_listed_for_it() {
    let (_dir, paths) = paths("one");
    let text = format!(
        r#"{{"LIVE": {{"swedish_(sweden)": {{"url": "{}"}}}}, "PTU": {{}}}}"#,
        address("LIVE", "swedish_(sweden)")
    );
    let net = Fake::default().with(LIST_URL, text);
    assert_eq!(refresh(&net, &paths), Refresh::Changed);
    assert_eq!(lists(&paths, "swedish_(sweden)", false), Some(true));
    assert_eq!(lists(&paths, "swedish_(sweden)", true), Some(false));
}

#[test]
fn names_and_ids_come_from_the_folder() {
    assert_eq!(id_of("swedish_(sweden)"), "luftwerft_swedish_sweden");
    assert_eq!(id_of("spanish_(latin_america)"), "luftwerft_spanish_latin_america");
    assert_eq!(name_of("swedish_(sweden)"), "Swedish, Sweden (Luftwerft)");
    assert_eq!(name_of("spanish_(latin_america)"), "Spanish, Latin America (Luftwerft)");
    assert_eq!(name_of("english"), "English (Luftwerft)");
    for fixed in FIXED {
        assert!(is_fixed(fixed), "{fixed}");
    }
    assert!(!is_fixed("swedish_(sweden)"));
}
