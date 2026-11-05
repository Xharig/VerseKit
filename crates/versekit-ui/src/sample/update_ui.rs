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

//! Updating from the window on the user's own data, without a screen: the button in the head, the bar, the
//! question, the progress, the page „Update & Über“ and the page „Was ist neu“. Everything runs against a made-up
//! net and a throwaway folder; no file of the program is replaced and no helper is started.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Event, Pos2};
use versekit_core::language::{Arg, t, tf};
use versekit_core::net::{FetchError, Net};
use versekit_core::paths::Paths;
use versekit_core::update_flow::Env;
use versekit_core::update_swap::Started;
use versekit_core::updater::{self, Asset, Cache, Release};

use super::controls::tests::Bench;
use super::docking::tests::draw;
use super::real_data::{host_in, real_page};

const BASE: &str = "https://github.com/Xharig/VerseKit/releases/download";
const IMAGE: &str = "VerseKit-x86_64.AppImage";

fn asset(version: &str, name: &str) -> Asset {
    Asset { name: name.into(), url: format!("{BASE}/{version}/{name}"), size: None }
}

fn release(version: &str, prerelease: bool) -> Release {
    Release {
        version: version.into(),
        date: "2026-11-20".into(),
        time: "2026-11-20T09:00:00Z".into(),
        prerelease,
        assets: vec![asset(version, IMAGE), asset(version, updater::CHECKSUM_FILE)],
        ..Release::default()
    }
}

fn write_cache(dir: &Path, releases: Vec<Release>) {
    let paths = Paths::new(host_in(dir, &[]));
    updater::cache_write(&paths, &Cache { checked: updater::now(), releases }).unwrap();
}

fn sha(bytes: &[u8]) -> String {
    let file = std::env::temp_dir().join(format!("versekit-sum-{}-{}", std::process::id(), bytes.len()));
    std::fs::write(&file, bytes).unwrap();
    let sum = updater::checksum_of(&file).unwrap();
    let _ = std::fs::remove_file(file);
    sum
}

/// GitHub with the release `version`: its AppImage and the checksums.
fn github(version: &str, content: &[u8]) -> versekit_core::net::Fake {
    versekit_core::net::Fake::default()
        .with(
            &format!("{BASE}/{version}/{}", updater::CHECKSUM_FILE),
            format!("{}  {IMAGE}\n", sha(content)).into_bytes(),
        )
        .with(&format!("{BASE}/{version}/{IMAGE}"), content.to_vec())
}

/// A net that stops in the middle of the download until the test lets it go.
struct Stalled {
    inner: versekit_core::net::Fake,
    gate: Mutex<std::sync::mpsc::Receiver<()>>,
}

impl Net for Stalled {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        self.inner.get(url)
    }

    fn head(&self, url: &str) -> Result<versekit_core::net::Headers, FetchError> {
        self.inner.head(url)
    }

    fn download(
        &self,
        url: &str,
        out: &mut dyn std::io::Write,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<u64, FetchError> {
        let body = self.inner.get(url)?;
        let (half, total) = (body.len() / 2, body.len() as u64);
        let wrote = |e: std::io::Error| FetchError::Failed(e.to_string());
        out.write_all(&body[..half]).map_err(wrote)?;
        progress(half as u64, Some(total));
        let _ = self.gate.lock().unwrap().recv_timeout(Duration::from_secs(30));
        out.write_all(&body[half..]).map_err(wrote)?;
        progress(total, Some(total));
        Ok(total)
    }
}

struct Lives;

impl Started for Lives {
    fn ended(&mut self) -> Option<i32> {
        None
    }
}

/// An AppImage of the throwaway folder, and the environment that fetches into it.
fn env_in(dir: &Path, net: Arc<dyn Net>, game: bool) -> (Env, PathBuf) {
    let image = dir.join(IMAGE);
    std::fs::write(&image, b"alt").unwrap();
    let env = Env {
        net: net.clone(),
        download_net: net,
        own_version: "5.0.0".into(),
        appimage: Some(image.to_string_lossy().into_owned()),
        program: image.clone(),
        temp: dir.to_path_buf(),
        start: Arc::new(|_| Ok(Box::new(Lives) as Box<dyn Started>)),
        game_running: Arc::new(move || game),
        alive_wait: Duration::from_millis(200),
    };
    (env, image)
}

/// A window on real data with `releases` remembered and the environment `env` (made by `make`).
fn window(name: &str, releases: Vec<Release>, settings: &str, make: impl FnOnce(&Path) -> Env) -> (Bench, PathBuf) {
    let (mut page, dir) = real_page(name, settings, &[]);
    write_cache(&dir, releases);
    page.use_updates(make(&dir));
    (Bench::new(page), dir)
}

fn offline_window(name: &str, releases: Vec<Release>) -> (Bench, PathBuf) {
    window(name, releases, "", |dir| env_in(dir, Arc::new(github("v5.0.1", b"neu")), false).0)
}

fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
    texts.iter().any(|(text, _)| text.contains(part))
}

/// Frames until `done` says yes, at most ten seconds.
fn pump(bench: &mut Bench, mut done: impl FnMut(&mut Bench, &[(String, Pos2)]) -> bool) -> Vec<(String, Pos2)> {
    let mut texts = bench.settle();
    for _ in 0..200 {
        if done(bench, &texts) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        texts = bench.settle();
    }
    texts
}

fn escape() -> Event {
    Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn the_button_in_the_head_is_grey_until_a_newer_version_is_known_and_asks_the_list_once_a_minute() {
    let _language = crate::language_lock();
    let (mut bench, dir) = offline_window("update-kopf", Vec::new());
    bench.time = Some(1.0);
    let grey = bench.settle();
    assert!(shows(&grey, &t("hf_update")) && !shows(&grey, &tf("hf_update_da", &[("0", Arg::from("5.0.1"))])));
    assert!(!shows(&grey, &tf("s_ul_da", &[("version", Arg::from("5.0.1"))])), "no bar without a version");
    // A newer version appears in the remembered list: the button does not look before a minute has passed.
    write_cache(&dir, vec![release("v5.0.1", false)]);
    bench.time = Some(30.0);
    assert!(!shows(&bench.settle(), &tf("hf_update_da", &[("0", Arg::from("5.0.1"))])), "not yet");
    bench.time = Some(62.0);
    let green = bench.settle();
    assert!(shows(&green, &tf("hf_update_da", &[("0", Arg::from("5.0.1"))])), "the number is on the button");
    assert!(shows(&green, &tf("s_ul_da", &[("version", Arg::from("5.0.1"))])), "and in the bar");
    // The way of the button leads to the page, as the list of 20.10. says.
    bench.click(&tf("hf_update_da", &[("0", Arg::from("5.0.1"))]));
    assert_eq!(bench.page.page_key(), "hf_ueber");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_test_version_is_only_offered_to_those_who_run_one_or_ask_for_them() {
    let _language = crate::language_lock();
    let releases = vec![release("v5.0.1-rc1", true)];
    let (mut bench, dir) = offline_window("update-kanal", releases.clone());
    let texts = bench.settle();
    assert!(!shows(&texts, &tf("hf_update_da", &[("0", Arg::from("5.0.1-rc1"))])), "the stable channel sees none");
    let (mut asking, dir2) = window("update-kanal-an", releases, "{\"vorabversionen\": true}", |dir| {
        env_in(dir, Arc::new(github("v5.0.1-rc1", b"neu")), false).0
    });
    assert!(
        shows(&asking.settle(), &tf("hf_update_da", &[("0", Arg::from("5.0.1-rc1"))])),
        "the setting asks for them"
    );
    let _ = (std::fs::remove_dir_all(dir), std::fs::remove_dir_all(dir2));
}

#[test]
fn the_bar_asks_before_it_installs_and_escape_and_a_click_beside_do_nothing() {
    let _language = crate::language_lock();
    let net = Arc::new(github("v5.0.1", b"neu"));
    let (mut bench, dir) =
        window("update-frage", vec![release("v5.0.1", false)], "", |dir| env_in(dir, net.clone(), false).0);
    let title = tf("hf_update_titel", &[("0", Arg::from("5.0.1"))]);
    bench.click(&t("s_ul_jetzt"));
    let texts = bench.settle();
    assert!(shows(&texts, &title), "the question has its title");
    assert!(shows(&texts, &tf("hf_update_frage", &[("0", Arg::from("5.0.1"))])));
    assert!(shows(&texts, &t("hf_update_jetzt")) && shows(&texts, &t("hf_update_spaeter")));
    assert!(
        !shows(&texts, &t("hf_update_frage_danach")) && !shows(&texts, &t("hf_update_danach")),
        "no game, no „after“"
    );
    assert!(net.asked().is_empty(), "nothing is fetched before the answer");
    bench.frame(vec![escape()]);
    assert!(shows(&bench.settle(), &title), "Escape does not close it");
    bench.click_at(Pos2::new(5.0, 5.0));
    assert!(shows(&bench.settle(), &title), "a click beside it does not close it");
    assert!(net.asked().is_empty() && !bench.page.updates.is_busy(), "and nothing was chosen");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn with_the_game_running_the_question_offers_the_other_way_after_the_game_and_installs_nothing_for_it() {
    let _language = crate::language_lock();
    let net = Arc::new(github("v5.0.1", b"neu"));
    let (mut bench, dir) =
        window("update-spiel", vec![release("v5.0.1", false)], "", |dir| env_in(dir, net.clone(), true).0);
    bench.click(&t("s_ul_jetzt"));
    let texts = bench.settle();
    assert!(shows(&texts, &tf("hf_update_frage_spiel", &[("0", Arg::from("5.0.1"))])));
    assert!(shows(&texts, &t("hf_update_frage_danach")), "it says what „after the game“ does");
    assert!(shows(&texts, &t("hf_update_danach")) && !shows(&texts, &t("hf_update_spaeter")));
    bench.click(&t("hf_update_danach"));
    assert_eq!(bench.notice(), Some(tf("hf_update_danach_ok", &[("0", Arg::from("5.0.1"))])));
    assert!(!shows(&bench.settle(), &t("hf_update_danach")), "the question is gone");
    assert!(net.asked().is_empty() && !bench.page.updates.is_busy(), "it installs nothing now");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn later_closes_the_question_without_a_word_and_without_fetching() {
    let _language = crate::language_lock();
    let net = Arc::new(github("v5.0.1", b"neu"));
    let (mut bench, dir) =
        window("update-spaeter", vec![release("v5.0.1", false)], "", |dir| env_in(dir, net.clone(), false).0);
    bench.click(&t("s_ul_jetzt"));
    bench.click(&t("hf_update_spaeter"));
    assert!(!shows(&bench.settle(), &t("hf_update_spaeter")));
    assert_eq!(bench.notice(), None);
    assert!(net.asked().is_empty() && !bench.page.updates.is_busy());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn install_now_fetches_shows_the_progress_and_ends_the_window_when_the_new_file_lives() {
    let _language = crate::language_lock();
    let (open, gate) = std::sync::mpsc::channel();
    // Forty bytes: half of them are exactly fifty percent.
    let stalled: Arc<dyn Net> = Arc::new(Stalled {
        inner: github("v5.0.1", b"0123456789012345678901234567890123456789"),
        gate: Mutex::new(gate),
    });
    let mut image = PathBuf::new();
    let (mut bench, dir) = window("update-jetzt", vec![release("v5.0.1", false)], "", |dir| {
        let (env, path) = env_in(dir, stalled.clone(), false);
        image = path;
        env
    });
    bench.click(&t("s_ul_jetzt"));
    bench.click(&t("hf_update_jetzt"));
    // Half of it is there: the foot bar says so, and the buttons that fetch wait.
    let half = "50 %".to_string();
    let texts = pump(&mut bench, |_, texts| shows(texts, &half));
    assert!(shows(&texts, &half), "the real percentage stands in the foot bar");
    assert!(shows(&texts, &tf("s_ub_holen_laeuft", &[("0", Arg::from("5.0.1"))])), "with the version it fetches");
    assert!(bench.page.updates.is_busy());
    assert!(!bench.page.take_update_quit(), "the window does not end before the file is in");
    // A second click on the bar only says that one is under way.
    bench.click(&t("s_ul_jetzt"));
    assert!(!shows(&bench.settle(), &t("hf_update_titel").replace("{0}", "")), "no second question");
    open.send(()).unwrap();
    pump(&mut bench, |bench, _| bench.page.update_quit);
    assert!(bench.page.take_update_quit(), "the program is to end");
    assert_eq!(std::fs::read(&image).unwrap(), b"0123456789012345678901234567890123456789");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_build_of_the_source_is_told_to_use_git_pull_and_asked_nothing() {
    let _language = crate::language_lock();
    let (mut bench, dir) = window("update-quelle", vec![release("v5.0.1", false)], "", |dir| {
        let mut env = env_in(dir, Arc::new(github("v5.0.1", b"neu")), false).0;
        env.own_version = "5.0.0-dev".into();
        env.appimage = None;
        env
    });
    bench.click(&t("s_ul_jetzt"));
    assert_eq!(bench.notice(), Some(t("update_quellcode")));
    assert_eq!(bench.page.page_key(), "hf_ueber");
    let texts = bench.settle();
    assert!(!shows(&texts, &t("hf_update_jetzt")), "no question");
    // The boxes name the newest version and offer no way „back“ to it from a build that is ahead of it.
    assert!(shows(&texts, &tf("s_ub_neueste_quellcode", &[("0", Arg::from("5.0.1"))])), "{texts:?}");
    assert!(!shows(&texts, &tf("s_ub_holen_zurueck", &[("0", Arg::from("5.0.1"))])), "no way back from the source");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_boxes_say_where_each_channel_leads_and_only_a_real_way_is_a_button() {
    let _language = crate::language_lock();
    let releases = vec![release("v5.0.1", false), release("v5.1.0-rc1", true)];
    let (mut bench, dir) = offline_window("update-kaesten", releases.clone());
    bench.page.go_to("hf_ueber");
    let texts = bench.settle();
    let fetch = |version: &str| tf("s_ub_holen", &[("0", Arg::from(version))]);
    assert!(shows(&texts, &fetch("5.0.1")) && shows(&texts, &fetch("5.1.0-rc1")), "both channels offer their newest");
    assert!(shows(&texts, &t("s_ub_fertig")) && shows(&texts, &t("s_ub_test")));
    // The running version in the channel: a statement, not a button. A click on it fetches nothing.
    let net = Arc::new(github("v5.0.1", b"neu"));
    let (mut same, dir2) = window("update-kaesten-gleich", releases.clone(), "", |dir| {
        let mut env = env_in(dir, net.clone(), false).0;
        env.own_version = "5.0.1".into();
        env
    });
    same.page.go_to("hf_ueber");
    let texts = same.settle();
    let equal = tf("s_ub_holen_gleich", &[("0", Arg::from("5.0.1"))]);
    assert!(shows(&texts, &equal), "{texts:?}");
    same.click(&equal);
    // Give a fetch that was started by mistake the time to show itself.
    std::thread::sleep(Duration::from_millis(400));
    same.settle();
    assert!(net.asked().is_empty(), "the running version is not fetched again: {:?}", net.asked());
    assert!(!same.page.updates.is_busy() && !same.page.update_quit);
    // A way back is named as one.
    let (mut back, dir3) = window("update-kaesten-zurueck", releases.clone(), "", |dir| {
        let mut env = env_in(dir, Arc::new(github("v5.0.1", b"neu")), false).0;
        env.own_version = "5.2.0".into();
        env
    });
    back.page.go_to("hf_ueber");
    assert!(shows(&back.settle(), &tf("s_ub_holen_zurueck", &[("0", Arg::from("5.0.1"))])));
    // Nothing known: a quiet sentence.
    let (mut none, dir4) = offline_window("update-kaesten-keine", Vec::new());
    none.page.go_to("hf_ueber");
    assert!(shows(&none.settle(), &t("s_ub_keine_bekannt")));
    for dir in [dir, dir2, dir3, dir4] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_click_on_a_box_chooses_the_channel_and_says_so_and_the_button_in_it_fetches() {
    let _language = crate::language_lock();
    let net = Arc::new(github("v5.0.1", b"new file"));
    let releases = vec![release("v5.0.1", false), release("v5.1.0-rc1", true)];
    let (mut bench, dir) = window("update-kanalwahl", releases, "", |dir| env_in(dir, net.clone(), false).0);
    bench.page.go_to("hf_ueber");
    assert!(!bench.page.update_test);
    bench.click(&t("s_ub_test"));
    assert!(bench.page.update_test, "the test channel is chosen");
    assert_eq!(bench.notice(), Some(format!("{}: {}", t("e_vorab"), t("e_an"))));
    bench.click(&t("s_ub_fertig"));
    assert!(!bench.page.update_test);
    assert_eq!(bench.notice(), Some(format!("{}: {}", t("e_vorab"), t("e_aus"))));
    assert!(!bench.page.updates.is_busy(), "choosing fetches nothing");
    bench.click(&tf("s_ub_holen", &[("0", Arg::from("5.0.1"))]));
    pump(&mut bench, |bench, _| bench.page.update_quit);
    assert!(bench.page.take_update_quit(), "the button in the box fetched at once");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_boxes_stand_side_by_side_in_a_wide_window_and_one_below_the_other_in_a_narrow_one() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("update-breite", "", &[]);
    write_cache(&dir, vec![release("v5.0.1", false)]);
    page.go_to("hf_ueber");
    let place = |texts: &[(String, Pos2)], text: &str| {
        texts.iter().find(|(t, _)| t == text).map(|(_, at)| *at).unwrap_or_else(|| panic!("{text} is not drawn"))
    };
    let (stable, test) = (t("s_ub_fertig"), t("s_ub_test"));
    let wide = draw(&mut page, 1500.0, 1000.0).1;
    let (a, b) = (place(&wide, &stable), place(&wide, &test));
    assert!((a.y - b.y).abs() < 3.0 && b.x > a.x + 100.0, "side by side: {a:?} {b:?}");
    let narrow = draw(&mut page, 800.0, 1000.0).1;
    let (a, b) = (place(&narrow, &stable), place(&narrow, &test));
    assert!(b.y > a.y + 40.0 && (a.x - b.x).abs() < 3.0, "one below the other: {a:?} {b:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_card_names_the_version_what_is_known_and_the_folder() {
    let _language = crate::language_lock();
    let (mut bench, dir) = offline_window("update-karte", Vec::new());
    bench.page.go_to("hf_ueber");
    let texts = bench.settle();
    for key in ["hf_titel", "s_ub_bekannt", "s_ub_davon", "b_ordner", "s_up_sofort", "s_up_sofort_h", "s_ub_kanal"] {
        assert!(shows(&texts, &t(key)), "{key}");
    }
    assert!(shows(&texts, "5.0.0"), "the version");
    let folder = Paths::new(host_in(&dir, &[])).app_folder();
    assert!(shows(&texts, &folder[..folder.len().min(20)]), "the folder");
    assert!(!shows(&texts, &t("s_ub_nachsehen")), "the button for looking is gone: the page looks by itself");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_page_looks_at_github_once_and_a_failed_look_is_never_up_to_date() {
    let _language = crate::language_lock();
    use versekit_core::legacy::ENV_NO_NET;
    // The net answers nothing: the look fails and the page says so, and never that all is up to date.
    let (mut page, dir) = real_page("update-blick-fehler", "", &[(ENV_NO_NET, "0")]);
    page.use_updates(env_in(&dir, Arc::new(versekit_core::net::Fake::default()), false).0);
    page.go_to("hf_ueber");
    let mut bench = Bench::new(page);
    bench.time = Some(1.0);
    let texts = pump(&mut bench, |_, texts| shows(texts, &t("s_ub_sucht_fehler")));
    assert!(shows(&texts, &t("s_ub_sucht_fehler")), "the failed look is told");
    assert!(!shows(&texts, &t("aktuelle_fassung")) && !shows(&texts, &t("s_ub_aktuell")));
    // With `SC_BP_NO_NET` the look is not made at all: no failure is told either.
    let (mut offline, dir2) = offline_window("update-blick-offline", Vec::new());
    offline.page.go_to("hf_ueber");
    offline.time = Some(1.0);
    assert!(!shows(&offline.settle(), &t("s_ub_sucht_fehler")));
    for dir in [dir, dir2] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn the_open_page_fills_its_boxes_from_the_look_and_repeats_it_every_five_minutes() {
    let _language = crate::language_lock();
    use versekit_core::legacy::ENV_NO_NET;
    let list = r#"[{"tag_name":"v5.0.1","published_at":"2026-11-20T09:00:00Z","prerelease":false,"assets":[]}]"#;
    let fake = Arc::new(versekit_core::net::Fake::default().with(updater::API, list.as_bytes().to_vec()));
    let (mut page, dir) = real_page("update-blick-kaesten", "", &[(ENV_NO_NET, "0")]);
    page.use_updates(env_in(&dir, fake.clone(), false).0);
    page.go_to("hf_ueber");
    let mut bench = Bench::new(page);
    let asked = |fake: &versekit_core::net::Fake| fake.asked().iter().filter(|u| u.as_str() == updater::API).count();
    bench.time = Some(1.0);
    let label = tf("s_ub_holen", &[("0", Arg::from("5.0.1"))]);
    pump(&mut bench, |_, texts| shows(texts, &label));
    assert!(shows(&bench.settle(), &label), "the boxes show what the look found");
    assert_eq!(asked(&fake), 1, "one look when the page opens");
    bench.time = Some(120.0);
    bench.settle();
    assert_eq!(asked(&fake), 1, "none within five minutes");
    bench.time = Some(310.0);
    pump(&mut bench, |_, _| asked(&fake) == 2);
    assert_eq!(asked(&fake), 2, "the next one after five minutes");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_look_tells_a_failure_and_the_hourly_limit_in_their_own_sentences() {
    let _language = crate::language_lock();
    use crate::update_page::Look;
    assert_eq!(Look { fetched: Some(true), rate_limited: false }.note(), None);
    assert_eq!(Look { fetched: None, rate_limited: false }.note(), None);
    let failed = Look { fetched: Some(false), rate_limited: false }.note();
    let limited = Look { fetched: Some(false), rate_limited: true }.note();
    assert_eq!(failed, Some(t("s_ub_sucht_fehler")));
    assert_eq!(limited, Some(t("s_ub_grenze")));
    assert_ne!(failed, limited);
}

/// Both pages in every size level, scheme and language, with data that are as long as they come: nothing is wider
/// than the window, the cards of a page are the width of the page, and the page draws its own first line.
#[test]
fn the_update_pages_fit_every_size_scheme_and_language() {
    use versekit_core::{font_size, theme};
    let _language = crate::language_lock();
    // A folder with a long path, a long release text with a word that cannot be broken.
    let (mut page, dir) = real_page(&"u".repeat(90), "", &[]);
    let mut long_release = release("v5.0.1", false);
    long_release.text = format!("### Added\n- {}\n- {}", "w".repeat(150), vec!["word"; 80].join(" "));
    write_cache(&dir, vec![long_release, release("v5.1.0-rc1", true)]);
    page.news.set_entries(vec![entry(
        "v4.2",
        &format!(
            "> {}\n### Added\n- {}\n### Fixed\n- {}",
            "l".repeat(120),
            "w".repeat(150),
            vec!["word"; 80].join(" ")
        ),
    )]);
    let mut findings = Vec::new();
    for scheme in theme::schemes() {
        theme::set_thread_current(scheme.id);
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            page.size = points;
            let (width, height) = font_size::min_window(points);
            for english in [false, true] {
                versekit_core::language::set_thread_language(if english {
                    versekit_core::language::Choice::En
                } else {
                    versekit_core::language::Choice::De
                });
                for (key, first_line) in [("hf_ueber", "s_up_sofort"), ("hf_wasistneu", "s_wn_lead")] {
                    page.go_to(key);
                    let (overflow, texts) = draw(&mut page, width, height.max(900.0));
                    let which = format!("{key} {} Größe {points} englisch {english}", scheme.id);
                    if overflow > 0.5 {
                        findings.push(format!("{which}: {overflow:.0} zu breit"));
                    }
                    // A scheme that writes its buttons in capitals still shows the line.
                    let wanted = t(first_line).to_lowercase();
                    if !texts.iter().any(|(text, _)| text.to_lowercase().contains(&wanted)) {
                        findings.push(format!("{which}: die Seite zeigt ihre erste Zeile nicht"));
                    }
                    let cards = super::docking::tests::draw_cards(&mut page, width, height.max(900.0));
                    let uneven = super::card_width::uneven(&cards);
                    if !uneven.is_empty() {
                        findings.push(format!("{which}: Kästen ungleich breit {uneven:?}"));
                    }
                }
            }
        }
    }
    theme::set_current("original");
    versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

#[test]
fn what_the_fetch_says_when_it_fails_reaches_the_player() {
    let _language = crate::language_lock();
    // No checksum file on the release: the fetch stops, and the sentence for the player is shown.
    let net = Arc::new(versekit_core::net::Fake::default().with(&format!("{BASE}/v5.0.1/{IMAGE}"), b"neu".to_vec()));
    let (mut bench, dir) =
        window("update-fehler", vec![release("v5.0.1", false)], "", |dir| env_in(dir, net.clone(), false).0);
    bench.click(&t("s_ul_jetzt"));
    bench.click(&t("hf_update_jetzt"));
    pump(&mut bench, |bench, _| bench.notice().is_some());
    let said = bench.notice().expect("the fetch said why it stopped");
    let wrapped = tf("update_fehler", &[("0", Arg::from(t("up_summen_netz")))]);
    assert_eq!(said, wrapped, "the sentence of the failure, in the words of the language file");
    assert!(!bench.page.update_quit);
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------------------ Was ist neu

fn entry(version: &str, text: &str) -> versekit_core::changelog::Entry {
    versekit_core::changelog::Entry {
        version: version.into(),
        date: "2026-10-19".into(),
        text: text.into(),
        built_in: true,
    }
}

const TEXT_ONE: &str = "> The lead of one\n### Added\n- **Bold** new thing with `code`\n### Fixed\n- one bug fixed\n### Changed\n- one thing better";
const TEXT_TWO: &str = "### Added\n- an old thing\n### Fixed\n- an old bug";
const TEXT_ONLY_NEW: &str = "### Added\n- only new here";

fn news_window(name: &str, entries: Vec<versekit_core::changelog::Entry>) -> (Bench, PathBuf) {
    let (mut bench, dir) = offline_window(name, Vec::new());
    bench.page.news.set_entries(entries);
    bench.page.go_to("hf_wasistneu");
    (bench, dir)
}

/// The button "Was ist neu" on the page Update & Über opens the gallery, although the gallery has no row in the
/// menu.
#[test]
fn the_news_button_of_the_about_page_opens_the_gallery() {
    let _language = crate::language_lock();
    let (mut bench, dir) = offline_window("news-knopf", Vec::new());
    bench.page.news.set_entries(vec![entry("v4.2", TEXT_ONE)]);
    bench.page.go_to("hf_ueber");
    let texts = bench.settle();
    assert!(shows(&texts, &t("s_up_sofort")), "the about page is open");
    bench.click(&t("hf_wasistneu"));
    let texts = bench.settle();
    assert_eq!(bench.page.page, "hf_wasistneu", "the gallery is the page now");
    assert!(shows(&texts, &t("s_wn_lead")), "the gallery shows its lead");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn only_the_newest_version_is_open_and_the_head_opens_and_closes_one() {
    let _language = crate::language_lock();
    let (mut bench, dir) =
        news_window("news-offen", vec![entry("v4.2", TEXT_ONE), entry("v4.1", TEXT_TWO), entry("v4.0", TEXT_ONLY_NEW)]);
    let texts = bench.settle();
    assert!(shows(&texts, &t("s_wn_lead")), "the page has its lead");
    for key in ["s_wn_f_alle", "s_wn_f_neu", "s_wn_f_bess", "s_wn_f_fix"] {
        assert!(shows(&texts, &t(key)), "the filter {key}");
    }
    assert!(shows(&texts, "The lead of one"), "the newest version is open and tells what it was about");
    assert!(shows(&texts, "Bold new thing with code"), "the signs of Markdown are gone");
    assert!(!shows(&texts, "an old thing") && !shows(&texts, "only new here"), "the others are closed");
    assert!(shows(&texts, tf("s_wn_aenderungen", &[("0", Arg::from(3_i64))]).trim()), "it counts its changes");
    // A click on the head of the second one opens it, another closes it.
    bench.click("v4.1");
    assert!(shows(&bench.settle(), "an old thing"));
    bench.click("v4.1");
    assert!(!shows(&bench.settle(), "an old thing"));
    // And the newest can be closed by hand.
    bench.click("v4.2");
    assert!(!shows(&bench.settle(), "The lead of one"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_newest_version_without_changes_does_not_leave_the_whole_page_closed() {
    let _language = crate::language_lock();
    let (mut bench, dir) = news_window("news-leer-erste", vec![entry("v4.3", "no list here"), entry("v4.2", TEXT_TWO)]);
    let texts = bench.settle();
    assert!(!shows(&texts, "v4.3"), "a version without a change is not listed");
    assert!(shows(&texts, "an old thing"), "the first one that is listed is open");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_filter_opens_every_version_shows_only_its_kind_and_drops_the_versions_without_any() {
    let _language = crate::language_lock();
    let (mut bench, dir) = news_window(
        "news-filter",
        vec![entry("v4.2", TEXT_ONE), entry("v4.1", TEXT_TWO), entry("v4.0", TEXT_ONLY_NEW)],
    );
    bench.click(&t("s_wn_f_fix"));
    let texts = bench.settle();
    assert!(shows(&texts, "one bug fixed") && shows(&texts, "an old bug"), "all versions are open for the filter");
    assert!(!shows(&texts, "new thing") && !shows(&texts, "only new here") && !shows(&texts, "one thing better"));
    assert!(!shows(&texts, "v4.0"), "a version without a fix is not listed");
    assert!(shows(&texts, &t("s_wn_aenderung_eins")), "the count is the filtered one");
    bench.click(&t("s_wn_f_alle"));
    let texts = bench.settle();
    assert!(shows(&texts, "v4.0") && !shows(&texts, "an old bug"), "everything again, only the newest open");
    // No version has a change of the kind: the page says so.
    let (mut empty, dir2) = news_window("news-leer", vec![entry("v4.0", TEXT_ONLY_NEW)]);
    empty.click(&t("s_wn_f_fix"));
    let texts = empty.settle();
    assert!(shows(&texts, &t("s_wn_nichts")) && !shows(&texts, "v4.0"));
    for dir in [dir, dir2] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn the_texts_beside_the_marks_start_in_one_line_and_at_most_twenty_five_versions_are_listed() {
    let _language = crate::language_lock();
    let (mut bench, dir) = news_window("news-spalte", vec![entry("v4.2", TEXT_ONE)]);
    bench.click(&t("s_wn_f_alle"));
    let texts = bench.settle();
    let column = |part: &str| texts.iter().find(|(t, _)| t.contains(part)).map(|(_, at)| at.x).unwrap();
    let (a, b, c) = (column("Bold new thing"), column("one bug fixed"), column("one thing better"));
    assert!((a - b).abs() < 0.5 && (b - c).abs() < 0.5, "the marks are equally wide: {a} {b} {c}");
    // And wide enough for the longest word: the text begins well to the right of where that word begins.
    // The filter button of the same word stands higher up; the mark is the lowest one.
    let word = t("s_wn_f_bess");
    let widest = texts
        .iter()
        .filter(|(text, _)| *text == word)
        .max_by(|a, b| a.1.y.total_cmp(&b.1.y))
        .map(|(_, at)| at.x)
        .unwrap();
    assert!(a - widest > 55.0, "the mark holds its word: text at {a}, word at {widest}");
    let many: Vec<_> = (0..40).map(|n| entry(&format!("v3.{}", 100 - n), TEXT_TWO)).collect();
    let (mut long, dir2) = news_window("news-viele", many);
    long.settle();
    assert_eq!(long.page.news.listed(), 25, "exactly the most the page lists, not forty");
    assert_eq!(versekit_core::changelog::MOST_ENTRIES, 25);
    for dir in [dir, dir2] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_long_line_breaks_inside_the_window_and_loses_no_word() {
    let _language = crate::language_lock();
    let words: Vec<String> = (0..60).map(|n| format!("word{n}")).collect();
    let line = format!("### Added\n- {}", words.join(" "));
    let (mut page, dir) = real_page("news-umbruch", "", &[]);
    write_cache(&dir, Vec::new());
    page.news.set_entries(vec![entry("v4.2", &line)]);
    page.go_to("hf_wasistneu");
    for width in [1500.0, 700.0] {
        let texts = draw(&mut page, width, 900.0).1;
        let shown: String =
            texts.iter().filter(|(t, _)| t.contains("word")).map(|(t, _)| t.clone()).collect::<Vec<_>>().join(" ");
        for word in &words {
            assert!(shown.contains(word.as_str()), "{word} is lost at {width}");
        }
    }
    // The words are in the drawn texts even when a line is cut short on the screen, so the height tells: sixty
    // words in a narrow window take many lines, and the card grows with them.
    let cards = super::docking::tests::draw_cards(&mut page, 700.0, 900.0);
    let tallest = cards.iter().map(|card| card.height()).fold(0.0_f32, f32::max);
    assert!(tallest > 120.0, "the long line wraps over several rows: card {tallest} high");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn on_real_data_the_page_reads_the_built_in_log_in_the_language_of_the_window() {
    let _language = crate::language_lock();
    let (mut bench, dir) = offline_window("news-echt", Vec::new());
    bench.page.go_to("hf_wasistneu");
    let german = bench.settle();
    assert!(shows(&german, &t("s_wn_lead")));
    // The series of the program itself (`v5.1` for 5.1.0) stands first.
    let version = env!("CARGO_PKG_VERSION");
    let series = format!("v{}", version.rsplit_once('.').map_or(version, |(series, _)| series));
    assert!(shows(&german, &series), "the series of the program is the first: {series}");
    versekit_core::language::set_thread_language(versekit_core::language::Choice::En);
    let english = bench.settle();
    assert!(shows(&english, &t("s_wn_lead")) && shows(&english, &series));
    let first = |lang| {
        let entries = versekit_core::changelog::grouped(lang, &[]);
        let points = versekit_core::changelog::points_by_kind(&entries[0].text);
        versekit_core::changelog::clean_row(&points[0].1)
    };
    let (de, en) = (first(versekit_core::language::Lang::De), first(versekit_core::language::Lang::En));
    assert_ne!(de, en, "the two logs differ");
    let start = |texts: &[(String, Pos2)], line: &str| shows(texts, &line[..line.len().min(25)]);
    assert!(start(&english, &en) && !start(&english, &de), "the English log is shown in English");
    versekit_core::language::set_thread_language(versekit_core::language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
}
