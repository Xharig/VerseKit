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

//! Sources, fetching and placing against a stand-in for the net, in a
//! made-up game folder in a throwaway folder.

use std::fs;

use super::*;
use crate::net::Fake;
use crate::paths::Host;
use crate::pypath::Flavor;

struct Game {
    root: String,
    paths: Paths,
    live: String,
    ptu: String,
}

impl Game {
    fn new(name: &str) -> Game {
        let root = std::env::temp_dir().join(format!("versekit-translation-{name}-{}", crate::testdir::run_tag()));
        let _ = fs::remove_dir_all(&root);
        let root = root.to_string_lossy().into_owned();
        let live = format!("{root}/StarCitizen/LIVE");
        let ptu = format!("{root}/StarCitizen/PTU");
        let data = format!("{root}/data");
        for folder in [&live, &ptu, &data] {
            fs::create_dir_all(folder).unwrap();
        }
        fs::write(format!("{live}/Game.log"), "").unwrap();
        fs::write(format!("{ptu}/Data.p4k"), "").unwrap();
        let host = Host::new(
            Flavor::Posix,
            &[("HOME", root.as_str()), ("SC_BP_HOME", &data), ("SC_INSTALL_DIR", &live)],
            &root,
        );
        Game { root, paths: Paths::new(host), live, ptu }
    }

    fn ini(&self, channel_folder: &str, language: &str) -> String {
        format!("{channel_folder}/data/Localization/{language}/global.ini")
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// A text file the game would take: vehicle names and many lines.
fn game_file(word: &str) -> Vec<u8> {
    let mut text = format!("\u{feff}Frontend_PU_Version={word}\nvehicle_NameAEGS_Gladius=Gladius\n");
    for i in 0..5001 {
        text.push_str(&format!("key_{i}={word} {i}\n"));
    }
    text.into_bytes()
}

fn no_progress(_: Progress) {}

/// A file written in text mode with `text`, as the system leaves it: on
/// Windows every line break is `\r\n`.
fn native(text: &str) -> String {
    text.replace('\n', if cfg!(windows) { "\r\n" } else { "\n" })
}

#[test]
fn every_source_knows_its_folder_and_the_test_versions_are_right() {
    let game = Game::new("sources");
    let ids: Vec<&str> = SOURCES.iter().map(|s| s.id).collect();
    let listed: Vec<&str> = game_ini::SOURCES.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, listed, "same sources as the lookup of the text file");
    for (id, folder) in game_ini::SOURCES {
        assert_eq!(language_folder(&game.paths, id, None), folder);
    }
    assert!(spec(&game.paths, "dymerz_de", Some("PTU")).is_none(), "a source without a test version has none");
    assert!(spec(&game.paths, "dymerz_de", Some("HOTFIX")).is_some(), "HOTFIX runs like LIVE");
    assert_eq!(spec(&game.paths, "starstrings", Some("PTU")).unwrap().tag.as_deref(), Some("latest-ptu"));
    assert!(spec(&game.paths, "luftwerft_fr", Some("PTU")).unwrap().url.unwrap().contains("/PTU/french_(france)/"));
    assert_eq!(language_folder(&game.paths, ORIGINAL, None), "english");
    // TECH-PREVIEW takes a translation like PTU, with the test version of the source.
    assert!(CHANNELS.contains(&"TECH-PREVIEW") && TEST_CHANNELS.contains(&"TECH-PREVIEW"));
    assert!(spec(&game.paths, "luftwerft_de", Some("TECH-PREVIEW")).unwrap().url.unwrap().contains("/PTU/german_"));
}

/// The channels that can follow LIVE are the test channels and HOTFIX; HOTFIX stays no test channel (it is a build
/// of LIVE: not confidential, the LIVE version of a source).
#[test]
fn hotfix_follows_live_and_stays_no_test_channel() {
    use crate::paths::FOLLOWS_LIVE;
    for channel in TEST_CHANNELS {
        assert!(FOLLOWS_LIVE.contains(&channel), "{channel}");
    }
    assert!(FOLLOWS_LIVE.contains(&"HOTFIX") && !TEST_CHANNELS.contains(&"HOTFIX"));
    assert_eq!(FOLLOWS_LIVE.len(), TEST_CHANNELS.len() + 1, "nothing else follows");
    assert!(!FOLLOWS_LIVE.contains(&"LIVE"));
    assert!(FOLLOWS_LIVE.iter().all(|channel| CHANNELS.contains(channel)));
}

#[test]
fn grouped_sources_like_4_12() {
    let game = Game::new("groups");
    let live = grouped_sources(&game.paths, None, true);
    let expected: Vec<Vec<&str>> = vec![
        vec!["luftwerft_de", "dymerz_de", "luftwerft_bar"],
        vec!["luftwerft_en", "starstrings", "original"],
        vec!["luftwerft_nl"],
        vec!["luftwerft_fr", "dymerz_fr"],
        vec!["luftwerft_it", "dymerz_it"],
        vec!["dymerz_pt"],
        vec!["luftwerft_pt"],
        vec!["luftwerft_es", "dymerz_es"],
        vec!["dymerz_tr"],
    ];
    assert_eq!(live, expected);
    let ptu = grouped_sources(&game.paths, Some("PTU"), true);
    let expected: Vec<Vec<&str>> = vec![
        vec!["luftwerft_de", "luftwerft_bar"],
        vec!["luftwerft_en", "starstrings", "original"],
        vec!["luftwerft_nl"],
        vec!["luftwerft_fr"],
        vec!["luftwerft_it"],
        vec!["luftwerft_pt"],
        vec!["luftwerft_es"],
    ];
    assert_eq!(ptu, expected);
}

#[test]
fn the_new_luftwerft_languages_come_for_both_channels_into_their_own_folders() {
    let game = Game::new("luftwerft-neu");
    let base = "https://www.luftwerft.com/download/resources/localizations";
    for (id, folder, label, flag) in [
        ("luftwerft_bar", "bavarian_(germany)", "Bayrisch (Luftwerft) [FUN]", "de"),
        ("luftwerft_nl", "dutch_(netherlands)", "Nederlands (Luftwerft)", "nl"),
        ("luftwerft_pt", "portuguese_(portugal)", "Português (Luftwerft)", "pt"),
    ] {
        let live = spec(&game.paths, id, None).expect(id);
        assert_eq!(live.url.as_deref(), Some(format!("{base}/LIVE/{folder}/global.ini").as_str()));
        assert_eq!(live.audio.as_deref(), Some("english"), "{id}: the voices stay English");
        let ptu = spec(&game.paths, id, Some("PTU")).expect(id);
        assert_eq!(ptu.url.as_deref(), Some(format!("{base}/PTU/{folder}/global.ini").as_str()));
        assert_eq!(spec(&game.paths, id, Some("EPTU")).unwrap().url, ptu.url, "{id}: EPTU takes the test version");
        assert_eq!(spec(&game.paths, id, Some("HOTFIX")).unwrap().url, live.url, "{id}: HOTFIX runs like LIVE");
        assert_eq!(language_folder(&game.paths, id, None), folder);
        assert_eq!(language_folder(&game.paths, id, Some("PTU")), folder);
        assert_eq!(flag_of(id), Some(flag));
        for lang in [Lang::De, Lang::En] {
            assert_eq!(language::t_in(lang, &label_key(id)), label, "{id} {lang:?}");
        }
        let target = game_ini::target_ini_in(&game.paths, &game.live, folder).expect(id);
        assert!(target.replace('\\', "/").ends_with(&format!("/Localization/{folder}/global.ini")), "{target}");
    }
}

#[test]
fn own_address_github_view_becomes_the_download() {
    assert_eq!(
        normalize_url(" https://github.com/a/b/blob/main/x/global.ini ").as_deref(),
        Some("https://raw.githubusercontent.com/a/b/main/x/global.ini")
    );
    assert_eq!(normalize_url("http://example.org/global.ini"), None);
    assert_eq!(normalize_url("HTTPS://x.org/a.zip").as_deref(), Some("HTTPS://x.org/a.zip"));
    let game = Game::new("custom");
    assert_eq!(set_custom(&game.paths, "ftp://x", "english", None), None);
    set_custom(&game.paths, "https://x.org/a.zip", "klingon", Some("PTU"));
    let entry = custom_settings(&game.paths, Some("PTU"));
    assert_eq!(entry.iter().find(|(k, _)| k == "sprache").and_then(|(_, v)| v.as_str()), Some("english"));
    assert!(custom_settings(&game.paths, None).is_empty());
}

#[test]
fn german_comes_from_a_release_and_is_placed_with_its_language_line() {
    let game = Game::new("german");
    let target = game.ini(&game.live, "german_(germany)");
    fs::create_dir_all(std::path::Path::new(&target).parent().unwrap()).unwrap();
    fs::write(&target, b"old text").unwrap();
    fs::write(format!("{}/user.cfg", game.live), "r_VSync = 0\ng_language = english\n").unwrap();
    let zip =
        crate::zip::tests::build(&[("data/Localization/german_(germany)/global.ini", &game_file("Deutsch"), true)]);
    let net = Fake::default()
        .with(
            "https://api.github.com/repos/Dymerz/StarCitizen-Localization/releases/latest",
            r#"{"tag_name": "v4.3.1", "published_at": "2026-09-16T04:52:00Z", "assets": [{"name": "german_.germany.zip", "browser_download_url": "https://dl/de.zip", "size": 10}]}"#,
        )
        .with("https://dl/de.zip", zip);
    let placed = fetch(&net, &game.paths, "dymerz_de", &game.live, None, &no_progress).unwrap();
    assert_eq!(placed.ident, "v4.3.1");
    assert_eq!(fs::read(&target).unwrap(), game_file("Deutsch"));
    assert_eq!(fs::read(placed.backup.as_ref().unwrap()).unwrap(), b"old text", "the old file is kept beside it");
    assert_eq!(
        fs::read_to_string(format!("{}/user.cfg", game.live)).unwrap(),
        native("r_VSync = 0\ng_language = german_(germany)\ng_languageAudio = english\n")
    );
    assert_eq!(
        fs::read_to_string(format!("{}/{USER_CFG_BACKUP}", game.live)).unwrap(),
        "r_VSync = 0\ng_language = english\n"
    );
    assert_eq!(installed(&game.paths, "dymerz_de", None).as_deref(), Some("v4.3.1"));
    assert!(injection::is_fresh(&game.paths), "a new base file is fresh");
    let lang = Lang::De;
    let today_text = status_text(&game.paths, "dymerz_de", None, lang, &today());
    assert!(
        today_text.starts_with("Stand 16.09.2026 · geholt heute ")
            && today_text.contains(" · aktuell, nachgesehen heute "),
        "{today_text}"
    );
}

/// The note of `key` with its fetch time `stand` replaced (`None` takes the field away).
fn set_fetched(game: &Game, key: &str, stand: Option<&str>) {
    let mut data = note(&game.paths);
    let Some((_, Value::Object(entry))) = data.iter_mut().find(|(k, _)| k == key) else { panic!("no note {key}") };
    entry.retain(|(k, _)| k != "stand");
    if let Some(stand) = stand {
        entry.push(("stand".into(), Value::str(stand)));
    }
    note_write(&game.paths, data);
}

#[test]
fn the_state_says_when_the_version_was_fetched_between_its_date_and_the_look() {
    let game = Game::new("geholt");
    note_set(&game.paths, "luftwerft_en", Some("etag:a"), Some("english"), Some("2026-09-16"));
    let state = |lang| status_text(&game.paths, "luftwerft_en", None, lang, &today());
    // Fetched today, like the look: both stand as "today <time>", in this order.
    let de = state(Lang::De);
    let (stand, fetched, looked) = (de.find("Stand "), de.find("geholt heute "), de.find("nachgesehen heute "));
    assert!(stand < fetched && fetched < looked && stand.is_some(), "{de}");
    let en = state(Lang::En);
    let (stand, fetched, looked) = (en.find("Version of "), en.find("fetched today "), en.find("checked today "));
    assert!(stand < fetched && fetched < looked && stand.is_some(), "{en}");
    // Fetched on an earlier day: the day and the time, written as the look is on another day.
    set_fetched(&game, "luftwerft_en", Some("2026-09-10 08:15"));
    assert!(state(Lang::De).contains("Stand 16.09.2026 · geholt 10.09.2026 08:15 · aktuell, nachgesehen heute "));
    assert!(state(Lang::En).contains("Version of 2026-09-16 · fetched 2026-09-10 08:15 · up to date, checked today "));
    // No time of fetching in the note: no such part, the rest as before.
    set_fetched(&game, "luftwerft_en", None);
    let none = state(Lang::De);
    assert!(!none.contains("geholt") && none.starts_with("Stand 16.09.2026 · aktuell, nachgesehen heute "), "{none}");
    // Without a date of the version the time of fetching comes first.
    note_set(&game.paths, "luftwerft_de", Some("etag:a"), Some("german_(germany)"), None);
    let first = status_text(&game.paths, "luftwerft_de", None, Lang::De, &today());
    assert!(first.starts_with("geholt heute "), "{first}");
}

#[test]
fn starstrings_test_version_hangs_on_its_fixed_tag() {
    let game = Game::new("starstrings");
    let zip = crate::zip::tests::build(&[("Data/Localization/english/global.ini", &game_file("SS"), true)]);
    let net = Fake::default()
        .with(
            "https://api.github.com/repos/MrKraken/StarStrings/releases/tags/latest-ptu",
            r#"{"tag_name": "latest-ptu", "published_at": "2026-09-16T19:48:42Z", "assets": [{"name": "StarStrings-PTU.zip", "browser_download_url": "https://dl/ptu.zip", "size": 10}]}"#,
        )
        .with("https://dl/ptu.zip", zip);
    let placed = fetch(&net, &game.paths, "starstrings", &game.ptu, Some("PTU"), &no_progress).unwrap();
    assert_eq!(placed.ident, "2026-09-16T19:48:42");
    assert_eq!(fs::read(game.ini(&game.ptu, "english")).unwrap(), game_file("SS"));
    assert_eq!(installed(&game.paths, "starstrings", Some("PTU")).as_deref(), Some("2026-09-16T19:48:42"));
    assert_eq!(installed(&game.paths, "starstrings", None), None, "the main installation is another note");
    assert!(!injection::is_fresh(&game.paths), "a side channel leaves the original texts of the main one");
}

#[test]
fn archives_without_a_folder_and_with_live_and_ptu_parts() {
    let plain = crate::zip::tests::build(&[("global.ini", b"KEY=x", true)]);
    assert_eq!(ini_from_zip(&plain, "french_(france)", None).unwrap(), (Some(b"KEY=x".to_vec()), None));
    let both = crate::zip::tests::build(&[
        ("LIVE/data/Localization/spanish_(spain)/global.ini", b"live", true),
        ("PTU/data/Localization/spanish_(spain)/global.ini", b"ptu", false),
    ]);
    assert_eq!(ini_from_zip(&both, "spanish_(spain)", Some("PTU/")).unwrap().0, Some(b"ptu".to_vec()));
    assert_eq!(ini_from_zip(&both, "spanish_(spain)", Some("LIVE/")).unwrap().0, Some(b"live".to_vec()));
    let other = crate::zip::tests::build(&[("Data/Localization/french_(france)/global.ini", b"fr", true)]);
    assert_eq!(ini_from_zip(&other, "english", None).unwrap(), (Some(b"fr".to_vec()), Some("french_(france)".into())));
}

#[test]
fn a_direct_address_takes_its_identifier_from_the_header_and_must_be_a_game_file() {
    let game = Game::new("direct");
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/german_(germany)/global.ini";
    let net = Fake::default().with_head(url, &[("ETag", "\"probe\"")]).with(url, game_file("LW"));
    let placed = fetch(&net, &game.paths, "luftwerft_de", &game.live, None, &no_progress).unwrap();
    assert_eq!(placed.ident, "etag:probe");

    set_custom(&game.paths, "https://example.org/page", "english", None);
    let page = Fake::default()
        .with_head("https://example.org/page", &[])
        .with("https://example.org/page", "<html><body>vehicle_ = </body></html>");
    assert_eq!(fetch(&page, &game.paths, CUSTOM, &game.live, None, &no_progress), Err(Problem::NotGameFile));
    let bare =
        Fake::default().with_head("https://example.org/page", &[]).with("https://example.org/page", game_file("Own"));
    let placed = fetch(&bare, &game.paths, CUSTOM, &game.live, None, &no_progress).unwrap();
    assert!(placed.ident.starts_with("inhalt:"), "without a header the content is the identifier");
}

#[test]
fn a_new_luftwerft_language_lands_in_its_folder_and_names_it_in_the_user_cfg() {
    let game = Game::new("luftwerft-ordner");
    let base = "https://www.luftwerft.com/download/resources/localizations";
    for (channel, folder) in [(None, &game.live), (Some("PTU"), &game.ptu)] {
        let part = channel.unwrap_or("LIVE");
        let url = format!("{base}/{part}/dutch_(netherlands)/global.ini");
        let net = Fake::default().with_head(&url, &[("ETag", "\"nl\"")]).with(&url, game_file("NL"));
        fetch(&net, &game.paths, "luftwerft_nl", folder, channel, &no_progress).unwrap();
        let placed = game.ini(folder, "dutch_(netherlands)");
        assert!(fs::read_to_string(&placed).unwrap().contains("Frontend_PU_Version=NL"), "{placed}");
        let cfg = fs::read_to_string(format!("{folder}/user.cfg")).unwrap();
        assert!(cfg.contains("g_language = dutch_(netherlands)"), "{part}: {cfg}");
        assert!(cfg.contains("g_languageAudio = english"), "{part}: {cfg}");
    }
}

/// The fetch holds the lock of the game files while it sets the language line: setting it must not take that lock
/// again (it is not taken twice by one thread and would wait for itself for ever).
#[test]
fn the_language_line_is_set_under_the_lock_of_the_fetch_without_waiting_for_itself() {
    let game = Game::new("sperre-verschachtelt");
    let (host, live) = (game.paths.host.clone(), game.live.clone());
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let paths = Paths::new(host);
        let held = injection::game_file_lock();
        let set = set_user_cfg(&paths, &live, "dutch_(netherlands)", Some("english"));
        drop(held);
        let _ = sender.send(set);
    });
    let set = receiver.recv_timeout(std::time::Duration::from_secs(30));
    assert_eq!(set, Ok(true), "the language line is set while the lock is held");
    let cfg = fs::read_to_string(format!("{}/user.cfg", game.live)).unwrap();
    assert!(cfg.contains("g_language = dutch_(netherlands)"), "{cfg}");
}

#[test]
fn update_check_notes_current_newer_and_failed() {
    let game = Game::new("check");
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/english/global.ini";
    note_set(&game.paths, "luftwerft_en", Some("etag:a"), Some("english"), None);
    let same = Fake::default().with_head(url, &[("ETag", "a")]);
    assert_eq!(update_available(&same, &game.paths, "luftwerft_en", None), (false, Some("etag:a".into())));
    assert_eq!(
        info(&game.paths, "luftwerft_en", None).iter().find(|(k, _)| k == "pruefung").and_then(|(_, v)| v.as_str()),
        Some("aktuell")
    );
    let newer = Fake::default().with_head(url, &[("ETag", "b")]);
    assert_eq!(update_available(&newer, &game.paths, "luftwerft_en", None), (true, Some("etag:b".into())));
    let down = Fake::default();
    assert_eq!(update_available(&down, &game.paths, "luftwerft_en", None), (false, None));
    assert_eq!(
        info(&game.paths, "luftwerft_en", None).iter().find(|(k, _)| k == "pruefung").and_then(|(_, v)| v.as_str()),
        Some("fehler")
    );
}

/// The state of a note as the pages read it, in German.
fn state_de(game: &Game, source: &str, channel: Option<&str>) -> String {
    status_text(&game.paths, source, channel, Lang::De, &today())
}

/// Python names of steps and the old start of a refused write; none may reach the player.
const INNER_NAMES: [&str; 8] =
    ["makedirs", "copy2", "replace", "remove", "rmdir", "mkdir", "Schreiben gesperrt", "os."];

#[test]
fn the_state_says_being_fetched_only_while_a_fetch_runs() {
    let game = Game::new("zustand");
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/english/global.ini";
    note_set(&game.paths, "luftwerft_en", Some("etag:a"), Some("english"), None);
    let newer = Fake::default().with_head(url, &[("ETag", "b")]).with(url, game_file("LW"));
    assert!(update_available(&newer, &game.paths, "luftwerft_en", None).0);
    // Found, but nothing fetches it: it says so, not "being fetched".
    let waiting = state_de(&game, "luftwerft_en", None);
    assert!(waiting.contains("neuere Fassung gefunden heute") && waiting.contains("noch nicht geholt"), "{waiting}");
    assert!(!waiting.contains("wird geholt"), "{waiting}");
    // While the fetch runs, the same note says "being fetched" (asked from inside the running fetch).
    let seen = std::cell::RefCell::new(Vec::new());
    let look = |_: Progress| seen.borrow_mut().push(state_de(&game, "luftwerft_en", None));
    fetch(&newer, &game.paths, "luftwerft_en", &game.live, None, &look).unwrap();
    let seen = seen.into_inner();
    assert!(!seen.is_empty() && seen.iter().all(|s| s.contains("wird geholt")), "{seen:?}");
    // Afterwards nothing runs any more: the placed version is noted, no "being fetched" stays behind.
    assert!(!fetching(&game.paths, "luftwerft_en"));
    assert!(!state_de(&game, "luftwerft_en", None).contains("wird geholt"));
    // Counter-check: the same running fetch of another note does not count for this one.
    let other = Fetching::start(&game.paths, "luftwerft_en@PTU");
    assert!(!fetching(&game.paths, "luftwerft_en") && fetching(&game.paths, "luftwerft_en@PTU"));
    drop(other);
}

#[test]
fn on_a_copy_of_the_data_only_a_look_is_taken_and_said_calmly() {
    let game = Game::new("testkopie-nachsehen");
    game.paths.set_given_data(&format!("{}/data", game.root), crate::given::GivenBy::CommandLine);
    assert!(crate::testmode::on_copy(&game.paths));
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/german_(germany)/global.ini";
    note_set(&game.paths, "luftwerft_de", Some("etag:a"), Some("german_(germany)"), Some("2026-09-01"));
    let net = Fake::default().with_head(url, &[("ETag", "b")]).with(url, game_file("LW"));
    assert_eq!(look_on_copy(&net, &game.paths, "luftwerft_de", None), Ok(Looked::Newer(None)));
    assert_eq!(net.asked(), [format!("HEAD {url}")], "only the header is asked, the file is not fetched");
    assert!(!std::path::Path::new(&game.ini(&game.live, "german_(germany)")).exists(), "nothing in the game");
    let state = state_de(&game, "luftwerft_de", None);
    assert!(state.contains("Neue Fassung, im Testbetrieb nicht eingespielt"), "{state}");
    assert!(!state.contains("gesperrt") && !state.contains("nicht geholt"), "no lock is told as a failure: {state}");
    assert_eq!(installed(&game.paths, "luftwerft_de", None).as_deref(), Some("etag:a"), "the note of the copy stays");

    let same = Fake::default().with_head(url, &[("ETag", "a")]);
    assert_eq!(look_on_copy(&same, &game.paths, "luftwerft_de", None), Ok(Looked::Current));
    assert!(state_de(&game, "luftwerft_de", None).contains("aktuell, nachgesehen heute"));

    // A source on GitHub names the day of the newer version.
    let release = "https://api.github.com/repos/MrKraken/StarStrings/releases/latest";
    note_set(&game.paths, "starstrings", Some("v1"), Some("english"), Some("2026-09-01"));
    let github = Fake::default().with(
        release,
        r#"{"tag_name": "v2", "published_at": "2026-10-28T09:00:00Z", "assets": [{"name": "StarStrings-LIVE.zip", "browser_download_url": "https://dl/live.zip", "size": 10}]}"#,
    );
    let looked = look_on_copy(&github, &game.paths, "starstrings", None);
    assert_eq!(looked, Ok(Looked::Newer(Some("2026-10-28".into()))));
    assert_eq!(github.asked(), [release], "only the release, no file");
    let state = state_de(&game, "starstrings", None);
    assert!(state.contains("Neue Fassung vom 28.10.2026, im Testbetrieb nicht eingespielt"), "{state}");
    for lang in [Lang::De, Lang::En] {
        let said = looked.clone().unwrap().text_in(lang);
        assert!(said.contains(if lang == Lang::De { "28.10.2026" } else { "2026-10-28" }), "{said}");
    }
}

#[test]
fn a_thread_with_the_lock_of_a_copy_only_looks_also_without_knowing_the_copy() {
    let game = Game::new("testkopie-sperre");
    let fresh = Paths::new(game.paths.host.clone());
    assert!(!crate::testmode::on_copy(&fresh), "paths built anew do not know the copy");
    assert!(!only_looks(&fresh, &game.live), "a normal run fetches");
    crate::writeguard::deny(true);
    crate::writeguard::allow_under(Some(&format!("{}/data", game.root)));
    let locked = only_looks(&fresh, &game.live);
    crate::writeguard::allow_also(&game.live);
    let inside = only_looks(&fresh, &game.live);
    crate::writeguard::deny(false);
    crate::writeguard::allow_under(None);
    let denied = crate::writeguard::take_denied();
    assert!(locked, "the game folder lies outside what the lock allows");
    assert!(!inside, "a game folder inside the copy may be written");
    assert!(denied.is_empty(), "asking records no refused write: {denied:?}");
}

#[test]
fn a_fetch_refused_on_a_copy_of_the_data_says_locked_and_writes_nothing_into_the_game() {
    let game = Game::new("gesperrt");
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/german_(germany)/global.ini";
    note_set(&game.paths, "luftwerft_de", Some("etag:a"), Some("german_(germany)"), None);
    let net = Fake::default().with_head(url, &[("ETag", "b")]).with(url, game_file("LW"));
    let target = game.ini(&game.live, "german_(germany)");
    // A run on a copy of the data: writing only in the data folder, the game folder is outside.
    crate::writeguard::deny(true);
    crate::writeguard::allow_under(Some(&format!("{}/data", game.root)));
    let newer = update_available(&net, &game.paths, "luftwerft_de", None).0;
    let result = fetch(&net, &game.paths, "luftwerft_de", &game.live, None, &no_progress);
    let denied = crate::writeguard::take_denied();
    crate::writeguard::deny(false);
    crate::writeguard::allow_under(None);
    assert!(newer);
    assert_eq!(result, Err(Problem::TestRun));
    assert!(!denied.is_empty() && !std::path::Path::new(&target).exists(), "nothing in the game: {denied:?}");
    for lang in [Lang::De, Lang::En] {
        let said = Problem::TestRun.text_in(lang);
        let state = status_text(&game.paths, "luftwerft_de", None, lang, &today());
        assert_eq!(said, language::t_in(lang, "schutz_spielordner"));
        let locked = if lang == Lang::De { "gesperrt: Im Testbetrieb" } else { "locked: In test mode" };
        assert!(state.contains(locked), "{state}");
        for text in [&said, &state] {
            assert!(text.contains("VerseKit") && !text.contains("wird geholt") && !text.contains("being fetched"));
            assert!(INNER_NAMES.iter().all(|name| !text.contains(name)), "{text}");
        }
    }
    assert!(state_de(&game, "luftwerft_de", None).contains("gesperrt: Im Testbetrieb schreibt VerseKit nicht"));
}

#[test]
fn a_failed_fetch_says_why_and_the_next_look_takes_its_place() {
    let game = Game::new("fehlgeschlagen");
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/english/global.ini";
    note_set(&game.paths, "luftwerft_en", Some("etag:a"), Some("english"), None);
    // The header names a newer version, the file itself does not come.
    let half = Fake::default().with_head(url, &[("ETag", "b")]);
    assert!(update_available(&half, &game.paths, "luftwerft_en", None).0);
    assert!(matches!(fetch(&half, &game.paths, "luftwerft_en", &game.live, None, &no_progress), Err(Problem::Net(_))));
    let state = state_de(&game, "luftwerft_en", None);
    assert!(state.contains("holen fehlgeschlagen heute") && state.contains(&language::t_in(Lang::De, "m_netz_fehl")));
    assert!(!state.contains("wird geholt"), "{state}");
    let english = status_text(&game.paths, "luftwerft_en", None, Lang::En, &today());
    assert!(english.contains("fetching failed today") && english.contains("could not be reached"), "{english}");
    // The next look is the state again, the reason of before goes.
    let same = Fake::default().with_head(url, &[("ETag", "a")]);
    update_available(&same, &game.paths, "luftwerft_en", None);
    let entry = info(&game.paths, "luftwerft_en", None);
    assert!(state_de(&game, "luftwerft_en", None).contains("aktuell, nachgesehen"));
    assert!(!entry.iter().any(|(k, _)| k == "grund"), "{entry:?}");
}

#[test]
fn every_reason_of_a_fetch_reads_as_a_sentence_and_comes_back_from_its_note() {
    let all = [
        Problem::NoChannelVersion("PTU".into()),
        Problem::NoVersion,
        Problem::Certificate,
        Problem::Refused,
        Problem::Offline,
        Problem::Net("tcp connect error: os error 10061".into()),
        Problem::NotGameFile,
        Problem::NoIniInArchive,
        Problem::NoGameFolder,
        Problem::TestRun,
        Problem::WriteFailed { denied: true },
        Problem::WriteFailed { denied: false },
    ];
    for problem in &all {
        let back = Problem::from_word(problem.word(), Some("PTU")).expect("known word");
        assert_eq!(back.short_text_in(Lang::De), problem.short_text_in(Lang::De), "{problem:?}");
        for lang in [Lang::De, Lang::En] {
            let text = problem.short_text_in(lang);
            assert!(!text.is_empty() && !text.starts_with('['), "{problem:?}: {text}");
            assert!(INNER_NAMES.iter().all(|name| !text.contains(name)), "{problem:?}: {text}");
            assert!(!text.contains("os error"), "{problem:?}: {text}");
        }
    }
    // A write the lock refused, one the system refused, and any other: three different sentences.
    let locked = crate::writeguard::Locked;
    assert_eq!(Problem::of_write(&std::io::Error::new(std::io::ErrorKind::PermissionDenied, locked)), Problem::TestRun);
    let system = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    assert_eq!(Problem::of_write(&system), Problem::WriteFailed { denied: true });
    assert_eq!(Problem::from_word("unbekannt", None), None);
}

#[test]
fn backups_are_cut_to_three_and_the_newest_comes_back() {
    let game = Game::new("backups");
    let target = game.ini(&game.live, "english");
    fs::create_dir_all(std::path::Path::new(&target).parent().unwrap()).unwrap();
    for i in 0..5 {
        fs::write(&target, format!("v{i}")).unwrap();
        backup_game_file(&game.paths, &target).unwrap();
        prune_backups(&target, BACKUPS_KEPT, None);
    }
    let all = backups_of(&target);
    assert_eq!(all.len(), 3);
    // Within one second the names still sort by age, with a counter.
    assert!(all.iter().all(|b| b.rsplit_once(BACKUP_MARK).is_some_and(|(_, tail)| is_backup_tail(tail))), "{all:?}");
    fs::write(&target, "fetched").unwrap();
    restore_backup(&game.paths, &target, true).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "v4");
    assert_eq!(backups_of(&target).len(), 2);
    assert!(!injection::is_fresh(&game.paths), "a restored file is not fresh");
}

#[test]
fn copies_of_the_older_version_are_found_and_put_back_but_none_is_made() {
    let game = Game::new("oldcopies");
    let target = game.ini(&game.live, "english");
    fs::create_dir_all(std::path::Path::new(&target).parent().unwrap()).unwrap();
    fs::write(format!("{target}{OLD_BACKUP_MARK}20200101-000001"), "from 4.12").unwrap();
    fs::write(&target, "before").unwrap();
    let made = replace_game_file(&game.paths, &target, b"fetched", BACKUPS_KEPT).unwrap().unwrap();
    assert!(made.contains(BACKUP_MARK), "{made}");
    let all = backups_of(&target);
    assert_eq!(all.len(), 2, "the old copy counts and stays: {all:?}");
    assert!(all[0].ends_with("20200101-000001"), "oldest first: {all:?}");
    // Back step by step: first the own copy, then the one of 4.12.
    restore_backup(&game.paths, &target, true).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "before");
    restore_backup(&game.paths, &target, true).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "from 4.12");
    assert!(backups_of(&target).is_empty());
    let folder = std::path::Path::new(&target).parent().unwrap();
    let left: Vec<String> =
        fs::read_dir(folder).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert!(left.iter().all(|n| !n.contains(OLD_BACKUP_MARK)), "no new trace of the old mark: {left:?}");
}

#[test]
fn cutting_never_removes_the_only_copy() {
    let game = Game::new("onlycopy");
    let target = game.ini(&game.live, "english");
    fs::create_dir_all(std::path::Path::new(&target).parent().unwrap()).unwrap();
    for i in 0..4 {
        fs::write(format!("{target}{OLD_BACKUP_MARK}2020010{i}-000000"), format!("old {i}")).unwrap();
    }
    fs::write(&target, "before").unwrap();
    replace_game_file(&game.paths, &target, b"fetched", 1).unwrap();
    let all = backups_of(&target);
    assert_eq!(all.len(), 1);
    assert_eq!(fs::read_to_string(&all[0]).unwrap(), "before", "the newest copy is the one kept");
}

#[test]
fn the_copy_just_made_stays_and_comes_last_also_when_the_clock_went_back() {
    let game = Game::new("clockback");
    let target = game.ini(&game.live, "english");
    fs::create_dir_all(std::path::Path::new(&target).parent().unwrap()).unwrap();
    // Three copies from "the future": the end of summer time, a changed zone.
    for i in 1..=3 {
        fs::write(format!("{target}{BACKUP_MARK}2999010{i}-000000"), format!("future {i}")).unwrap();
    }
    fs::write(&target, "before").unwrap();
    let made = replace_game_file(&game.paths, &target, b"fetched", 3).unwrap().unwrap();
    assert!(fs::metadata(&made).is_ok(), "the copy just made was cut at once: {made}");
    let all = backups_of(&target);
    assert_eq!(all.len(), 3, "{all:?}");
    assert_eq!(all.last().map(|b| file_name(b)), Some(file_name(&made)), "it counts as the newest: {all:?}");
    assert_eq!(fs::read_to_string(&made).unwrap(), "before");
    restore_backup(&game.paths, &target, false).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "before", "the newest one comes back, not a future one");
}

#[test]
fn the_user_cfg_keeps_every_other_line_and_takes_known_languages_only() {
    let game = Game::new("usercfg");
    let cfg = format!("{}/user.cfg", game.live);
    assert!(set_user_cfg(&game.paths, &game.live, "english", None));
    assert_eq!(fs::read_to_string(&cfg).unwrap(), native("g_language = english\n"));
    assert!(!game_ini::is_file(&format!("{}/{USER_CFG_BACKUP}", game.live)), "nothing to keep from a new file");
    fs::write(&cfg, "sys_MaxFPS = 60\ng_languageAudio = english\n").unwrap();
    assert!(set_user_cfg(&game.paths, &game.live, "german_(germany)", None));
    assert_eq!(
        fs::read_to_string(&cfg).unwrap(),
        native("sys_MaxFPS = 60\ng_languageAudio = english\ng_language = german_(germany)\n")
    );
    assert!(!set_user_cfg(&game.paths, &game.live, "german\ng_x = 1", None), "a value with a line break is refused");
    assert_eq!(game_ini::game_language_in(&game.paths, &game.live).as_deref(), Some("german_(germany)"));
    fs::write(&cfg, "changed by hand\n").unwrap();
    set_user_cfg(&game.paths, &game.live, "english", None);
    assert_eq!(
        fs::read_to_string(format!("{}/{USER_CFG_BACKUP}", game.live)).unwrap(),
        "sys_MaxFPS = 60\ng_languageAudio = english\n",
        "the first copy is never overwritten"
    );
}

#[test]
fn installed_channels_need_only_their_folder() {
    let game = Game::new("channels");
    let found = game.paths.installed_channels();
    let names: Vec<(&str, bool)> = found.iter().map(|(n, _, p4k)| (n.as_str(), *p4k)).collect();
    assert_eq!(names, [("LIVE", false), ("PTU", true)]);
}

/// The real archive of a source, fetched beforehand by hand, gives the same
/// file as a common zip tool: `VERSEKIT_ZIP=<archive> VERSEKIT_ZIP_INI=<unpacked global.ini>
/// cargo test -p versekit-core real_archive -- --ignored`.
#[test]
#[ignore = "needs an archive fetched by hand"]
fn a_real_archive_unpacks_like_a_zip_tool() {
    let archive = fs::read(std::env::var("VERSEKIT_ZIP").expect("VERSEKIT_ZIP")).unwrap();
    let expected = fs::read(std::env::var("VERSEKIT_ZIP_INI").expect("VERSEKIT_ZIP_INI")).unwrap();
    let (ini, folder) = ini_from_zip(&archive, "english", None).unwrap();
    assert_eq!(folder.as_deref(), Some("english"));
    assert_eq!(ini.as_deref(), Some(expected.as_slice()));
    assert!(looks_like_global_ini(&expected));
}

#[test]
fn a_retired_source_moves_to_its_successor_with_its_note_and_is_fetched_anew() {
    let game = Game::new("retired");
    let (old, new) = RETIRED_SOURCES[0];
    game.paths.set_setting(SETTING_SOURCE, Value::str(old));
    set_channel_source(&game.paths, "HOTFIX", old);
    note_set(&game.paths, old, Some("git-0123456789ab"), Some("german_(germany)"), Some("2026-09-01"));
    note_set(&game.paths, &note_key(old, Some("HOTFIX")), Some("git-1"), Some("german_(germany)"), None);
    game.paths.set_setting(crate::game_texts::SETTING_CONFIRMED, Value::str(format!("{old},starstrings")));
    assert!(retire_sources(&game.paths));
    assert_eq!(game.paths.setting(SETTING_SOURCE).as_deref(), Some(new));
    let channels = channel_sources(&game.paths);
    assert_eq!(channels.iter().find(|(k, _)| k == "HOTFIX").and_then(|(_, v)| v.as_str()), Some(new));
    assert!(note(&game.paths).iter().all(|(k, _)| k != old && !k.starts_with(&format!("{old}@"))), "no old note stays");
    assert_eq!(installed(&game.paths, new, None).as_deref(), Some("git-0123456789ab"), "the old file counts as placed");
    assert_eq!(installed(&game.paths, new, Some("HOTFIX")).as_deref(), Some("git-1"));
    assert!(info(&game.paths, new, None).iter().all(|(k, _)| k != "datum"), "the day of the old version goes");
    assert_eq!(game.paths.setting(crate::game_texts::SETTING_CONFIRMED).as_deref(), Some("starstrings"));
    assert_eq!(crate::game_texts::active_source(&game.paths).as_deref(), Some(new));
    // The next look finds the file of the successor newer than the old one and fetches it.
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/german_(germany)/global.ini";
    let net = Fake::default().with_head(url, &[("ETag", "\"lw\"")]);
    assert_eq!(update_available(&net, &game.paths, new, None), (true, Some("etag:lw".into())));
    assert!(!retire_sources(&game.paths), "a second start finds nothing");
}

/// Español of Thord82 is no longer offered: who chose it, for LIVE or for the PTU, gets Español of Luftwerft into
/// the same folder, and the next look fetches its file.
#[test]
fn the_spanish_of_thord82_moves_to_the_spanish_of_luftwerft() {
    let game = Game::new("retired-thord82");
    assert!(source("thord82_es").is_none(), "no longer offered");
    assert!(grouped_sources(&game.paths, None, true).iter().flatten().all(|id| id != "thord82_es"));
    game.paths.set_setting(SETTING_SOURCE, Value::str("thord82_es"));
    set_channel_source(&game.paths, "PTU", "thord82_es");
    note_set(&game.paths, "thord82_es", Some("v4.10"), Some("spanish_(spain)"), Some("2026-10-01"));
    note_set(&game.paths, &note_key("thord82_es", Some("PTU")), Some("v4.11"), Some("spanish_(spain)"), None);
    game.paths.set_setting(crate::game_texts::SETTING_CONFIRMED, Value::str("thord82_es,starstrings"));
    assert!(retire_sources(&game.paths));
    assert_eq!(game.paths.setting(SETTING_SOURCE).as_deref(), Some("luftwerft_es"));
    let channels = channel_sources(&game.paths);
    assert_eq!(channels.iter().find(|(k, _)| k == "PTU").and_then(|(_, v)| v.as_str()), Some("luftwerft_es"));
    assert!(note(&game.paths).iter().all(|(k, _)| !k.starts_with("thord82_es")), "no old note stays");
    assert_eq!(installed(&game.paths, "luftwerft_es", None).as_deref(), Some("v4.10"));
    assert_eq!(installed(&game.paths, "luftwerft_es", Some("PTU")).as_deref(), Some("v4.11"));
    assert_eq!(language_folder(&game.paths, "luftwerft_es", None), "spanish_(spain)", "the same folder as before");
    assert_eq!(game.paths.setting(crate::game_texts::SETTING_CONFIRMED).as_deref(), Some("starstrings"));
    assert_eq!(crate::game_texts::active_source(&game.paths).as_deref(), Some("luftwerft_es"));
    let url = "https://www.luftwerft.com/download/resources/localizations/LIVE/spanish_(spain)/global.ini";
    let net = Fake::default().with_head(url, &[("ETag", "\"lw\"")]);
    assert_eq!(update_available(&net, &game.paths, "luftwerft_es", None), (true, Some("etag:lw".into())));
    assert!(!retire_sources(&game.paths), "a second start finds nothing");
}

#[test]
fn a_retired_note_replaces_the_successor_only_where_it_was_chosen() {
    let game = Game::new("retired-taken");
    let (old, new) = RETIRED_SOURCES[0];
    note_set(&game.paths, old, Some("git-1"), Some("german_(germany)"), None);
    note_set(&game.paths, new, Some("etag:lw"), Some("german_(germany)"), None);
    game.paths.set_setting(SETTING_SOURCE, Value::str(new));
    assert!(retire_sources(&game.paths));
    assert_eq!(installed(&game.paths, new, None).as_deref(), Some("etag:lw"), "the chosen successor keeps its note");
    assert_eq!(installed(&game.paths, old, None), None);
    note_set(&game.paths, old, Some("git-1"), Some("german_(germany)"), None);
    game.paths.set_setting(SETTING_SOURCE, Value::str(old));
    assert!(retire_sources(&game.paths));
    assert_eq!(installed(&game.paths, new, None).as_deref(), Some("git-1"), "the file of the chosen old source counts");
}
