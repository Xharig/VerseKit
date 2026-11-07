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

//! The checks of the page "Shader-Cache": every one runs on a made-up tree in a throw-away folder; nothing real is
//! ever emptied.

use std::path::Path;

use egui::{Event, Pos2, Rect};
use versekit_core::language::{self, Choice};
use versekit_core::paths::{ENV_INSTALL_DIR, Host};
use versekit_core::pypath::Flavor;
use versekit_core::theme;

use super::*;

const MIB: usize = 1024 * 1024;

fn scratch(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("versekit-shaderpage-{}-{name}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    dir.to_string_lossy().replace('\\', "/")
}

fn put(path: &str, bytes: usize) {
    std::fs::create_dir_all(Path::new(path).parent().expect("parent")).expect("folder");
    std::fs::write(path, vec![1u8; bytes]).expect("file");
}

/// A user on Linux with the game and a cache tree: the version 4.1 (3 MB), the version 4.0 (5 MB), a cache of
/// NVIDIA (2 MB) and crash reports (1 MB).
struct World {
    paths: Paths,
    home: String,
    /// A file in the folder of 4.1.
    current: String,
    /// A file in the folder of 4.0.
    old: String,
    driver: String,
    crash: String,
    /// The graphics settings of 4.1.
    settings: String,
    /// The Game.log, which the tests touch.
    log: String,
}

/// The Game.log is `age_secs` old (`None`: no game folder at all); the installed branch is `branch` (`None`: the
/// channel has no `build_manifest.id`).
fn world(name: &str, age_secs: Option<u64>, branch: Option<&str>) -> World {
    let home = scratch(name);
    let local = format!("{home}/Games/star-citizen/drive_c/users/ich/AppData/Local");
    let current = format!("{local}/Star Citizen/starcitizen_(sc-alpha-4.1)_x/shaders/a.bin");
    let old = format!("{local}/Star Citizen/starcitizen_(sc-alpha-4.0)_y/shaders/b.bin");
    let driver = format!("{home}/.cache/nvidia/GLCache/g.bin");
    let crash = format!("{local}/Star Citizen/crashes/e.dmp");
    put(&current, 3 * MIB);
    put(&old, 5 * MIB);
    put(&driver, 2 * MIB);
    put(&crash, MIB);
    let settings = format!("{local}/Star Citizen/starcitizen_(sc-alpha-4.1)_x/GraphicsSettings/GraphicsSettings.json");
    put(&settings, 50);
    // The real process list is not asked: a test must not depend on a game that runs on this computer.
    let mut vars = vec![("HOME", home.as_str()), (game_start::ENV_NO_PROCESS, "1")];
    let game = format!("{home}/Games/star-citizen/drive_c/Program Files/Roberts Space Industries/StarCitizen/LIVE");
    let log = format!("{game}/Game.log");
    if let Some(age) = age_secs {
        put(&log, 10);
        put(&format!("{game}/Data.p4k"), 10);
        if let Some(branch) = branch {
            std::fs::write(format!("{game}/build_manifest.id"), format!(r#"{{"Data": {{"Branch": "{branch}"}}}}"#))
                .expect("manifest");
        }
        let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options().write(true).open(&log).and_then(|f| f.set_modified(at)).expect("age");
        vars.push((ENV_INSTALL_DIR, game.as_str()));
    }
    let paths = Paths::new(Host::new(Flavor::Posix, &vars, &home));
    World { paths, home, current, old, driver, crash, settings, log }
}

type Texts = Vec<(String, Pos2)>;

struct Bench {
    ctx: egui::Context,
    state: ShaderState,
    size: egui::Vec2,
    /// Every named element of the last frame with its place.
    nodes: Vec<(egui::Role, String, Rect)>,
    /// The named elements of the last frame that do nothing now (grey).
    grey: Vec<(egui::Role, String)>,
}

impl Bench {
    fn new() -> Bench {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        Bench {
            ctx,
            state: ShaderState::default(),
            size: egui::vec2(1000.0, 700.0),
            nodes: Vec::new(),
            grey: Vec::new(),
        }
    }

    fn frame(&mut self, paths: &Paths, events: Vec<Event>) -> Texts {
        let scheme = theme::by_id("original").expect("original");
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            events,
            ..Default::default()
        };
        let state = &mut self.state;
        let mut output = self.ctx.run_ui(input, |ui| show_page(ui, scheme, state, paths));
        output.textures_delta.clear();
        let mut texts = Vec::new();
        fn collect(shape: &egui::Shape, into: &mut Texts) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                _ => {}
            }
        }
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        let update = output.platform_output.accesskit_update;
        self.grey = update
            .as_ref()
            .map(|update| {
                update
                    .nodes
                    .iter()
                    .filter(|(_, node)| node.is_disabled())
                    .map(|(_, node)| (node.role(), node.label().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        self.nodes = update
            .map(|update| {
                update
                    .nodes
                    .iter()
                    .filter_map(|(_, node)| {
                        let b = node.bounds()?;
                        let corner = |x: f64, y: f64| Pos2::new(x as f32, y as f32);
                        let rect = Rect::from_min_max(corner(b.x0, b.y0), corner(b.x1, b.y1));
                        Some((node.role(), node.label().unwrap_or_default().to_string(), rect))
                    })
                    .collect()
            })
            .unwrap_or_default();
        texts
    }

    fn settle(&mut self, paths: &Paths) -> Texts {
        let mut texts = Vec::new();
        for _ in 0..4 {
            texts = self.frame(paths, Vec::new());
        }
        texts
    }

    /// Frames until `done` is true, at most three seconds.
    fn until(&mut self, paths: &Paths, done: impl Fn(&Texts) -> bool) -> Texts {
        let mut texts = self.settle(paths);
        for _ in 0..60 {
            if done(&texts) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            texts = self.settle(paths);
        }
        texts
    }

    fn press_at(&mut self, paths: &Paths, at: Pos2) {
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(paths, vec![Event::PointerMoved(at)]);
        self.frame(paths, vec![button(true)]);
        self.frame(paths, vec![button(false)]);
        self.frame(paths, Vec::new());
    }

    fn click(&mut self, paths: &Paths, label: &str) {
        let texts = self.settle(paths);
        let at = texts
            .iter()
            .find(|(text, _)| text == label)
            .map(|(_, at)| *at + egui::vec2(3.0, 3.0))
            .unwrap_or_else(|| panic!("{label} is not on the screen: {texts:?}"));
        self.press_at(paths, at);
    }

    /// The place of the named element of that kind.
    fn place(&mut self, paths: &Paths, role: egui::Role, name: &str) -> Rect {
        let texts = self.settle(paths);
        self.nodes
            .iter()
            .find(|(r, n, _)| *r == role && n == name)
            .map(|(_, _, rect)| *rect)
            .unwrap_or_else(|| panic!("no {role:?} {name} on the screen: {texts:?} {:?}", self.nodes))
    }

    /// Whether the named element of that kind stands on the screen and does something (is not grey).
    fn enabled(&self, role: egui::Role, name: &str) -> bool {
        self.nodes.iter().any(|(r, n, _)| *r == role && n == name)
            && !self.grey.iter().any(|(r, n)| *r == role && n == name)
    }

    fn switch(&mut self, paths: &Paths, name: &str) {
        let at = self.place(paths, egui::Role::CheckBox, name).center();
        self.press_at(paths, at);
    }
}

fn shows(texts: &Texts, text: &str) -> bool {
    texts.iter().any(|(t, _)| t.contains(text))
}

fn setup() -> std::sync::MutexGuard<'static, ()> {
    let guard = crate::language_lock();
    theme::set_current("original");
    language::set_language(Choice::De);
    guard
}

fn name_current() -> String {
    tf("s_sc_ort_sc_n", &[("0", Arg::from("4.1")), ("1", Arg::from("x"))])
}

fn name_old() -> String {
    tf("s_sc_ort_sc_n", &[("0", Arg::from("4.0")), ("1", Arg::from("y"))])
}

fn size_of(mib: usize) -> String {
    shader_cache::format_size((mib * MIB) as u64, ',')
}

/// Starts a way, waits for the question, and returns what it shows.
fn ask(bench: &mut Bench, paths: &Paths, button: &str) -> Texts {
    bench.click(paths, &t(button));
    bench.until(paths, |texts| shows(texts, &t("s_mlg_bestaetigen")))
}

/// The card says how large the caches are and when they were last emptied through VerseKit; emptied before the last
/// patch of the game (its archive is newer), it says so; emptying through the card keeps the moment.
#[test]
fn the_card_tells_the_size_when_it_was_last_cleared_and_whether_that_was_before_the_patch() {
    let _language = setup();
    let w = world("groesse", Some(7200), Some("sc-alpha-4.1"));
    let mut bench = Bench::new();
    // 11 MB in all, never emptied through VerseKit, nothing to say about the patch.
    let size = tf("s_sc_groesse", &[("0", Arg::from(size_of(11).as_str()))]);
    let texts = bench.until(&w.paths, |texts| shows(texts, &size));
    assert!(shows(&texts, &size) && shows(&texts, &t("s_sc_nie_geleert")), "{texts:?}");
    assert!(!shows(&texts, &t("s_sc_seit_patch")), "{texts:?}");
    // Emptied long before the archive of the game was written: said so, with the day.
    let game = w.paths.game_folder().expect("a game folder");
    std::fs::write(format!("{game}/Data.p4k"), "").expect("archive");
    w.paths.set_setting(SETTING_CLEARED, Value::int(1_000_000));
    let texts = bench.settle(&w.paths);
    assert!(shows(&texts, &t("s_sc_seit_patch")), "{texts:?}");
    let day = versekit_core::stats_report::local_day(versekit_core::language::current(), 1_000_000);
    assert!(shows(&texts, &tf("s_sc_geleert_am", &[("0", Arg::from(day.as_str()))])), "{texts:?}");
    // Counter-check: emptied after the patch, nothing said.
    w.paths.set_setting(SETTING_CLEARED, Value::int(versekit_core::export::now_unix() + 60));
    let texts = bench.settle(&w.paths);
    assert!(!shows(&texts, &t("s_sc_seit_patch")), "{texts:?}");
    // Emptying through the card keeps the moment it was done.
    w.paths.set_setting(SETTING_CLEARED, Value::int(1_000_000));
    ask(&mut bench, &w.paths, "s_sc_knopf");
    bench.click(&w.paths, &t("s_mlg_bestaetigen"));
    let done = tf("s_sc_fertig", &[("0", Arg::from(size_of(11).as_str()))]);
    bench.until(&w.paths, |texts| shows(texts, &done));
    assert!(cleared_at(&w.paths).is_some_and(|at| at >= versekit_core::export::now_unix() - 60), "kept");
    let _ = std::fs::remove_dir_all(w.home);
}

#[test]
fn clearing_everything_asks_with_a_switch_for_each_place_and_tells_the_result_for_each() {
    let _language = setup();
    let w = world("alles", Some(7200), Some("sc-alpha-4.1"));
    let mut bench = Bench::new();
    let texts = ask(&mut bench, &w.paths, "s_sc_knopf");
    // Every place is in the list with its size; the switches stand on.
    assert!(shows(&texts, &t("s_sc_frage_kopf")), "{texts:?}");
    for name in [name_current(), name_old(), t("s_sc_ort_nvidia"), t("s_sc_ort_crash")] {
        assert!(shows(&texts, &name), "{name}: {texts:?}");
    }
    assert!(shows(&texts, &tf("s_sc_gewaehlt", &[("0", Arg::from(size_of(11).as_str()))])), "{texts:?}");
    assert!(Path::new(&w.old).is_file(), "nothing is deleted before the yes");
    // Not now: nothing deleted either.
    bench.click(&w.paths, &t("abbrechen"));
    assert!(bench.state.phase_is_idle());
    assert!(Path::new(&w.old).is_file() && Path::new(&w.current).is_file());
    // Again, and yes.
    ask(&mut bench, &w.paths, "s_sc_knopf");
    bench.click(&w.paths, &t("s_mlg_bestaetigen"));
    let done = tf("s_sc_fertig", &[("0", Arg::from(size_of(11).as_str()))]);
    let texts = bench.until(&w.paths, |texts| shows(texts, &done));
    assert!(shows(&texts, &done), "the window tells what was freed: {texts:?}");
    let line = |name: &str, mib: usize| {
        tf("s_sc_erg_zeile", &[("0", Arg::from(name)), ("1", Arg::from(size_of(mib).as_str()))])
    };
    for (name, mib) in [(name_current(), 3), (name_old(), 5), (t("s_sc_ort_nvidia"), 2), (t("s_sc_ort_crash"), 1)] {
        assert!(shows(&texts, &line(&name, mib)), "{name}: {texts:?}");
    }
    for file in [&w.current, &w.old, &w.driver, &w.crash] {
        assert!(!Path::new(file).exists(), "{file}: the cache is emptied");
        assert!(Path::new(file).parent().is_some_and(Path::is_dir), "{file}: the folder stays");
    }
    assert!(Path::new(&w.settings).is_file(), "the graphics settings stay");
    let _ = std::fs::remove_dir_all(w.home);
}

#[test]
fn a_place_switched_off_stays_and_with_every_place_off_the_yes_does_nothing() {
    let _language = setup();
    let w = world("einzeln", Some(7200), Some("sc-alpha-4.1"));
    let mut bench = Bench::new();
    ask(&mut bench, &w.paths, "s_sc_knopf");
    // The mouse switches two places off: the cache of the installed version and the driver.
    bench.switch(&w.paths, &name_current());
    bench.switch(&w.paths, &t("s_sc_ort_nvidia"));
    let texts = bench.settle(&w.paths);
    assert!(shows(&texts, &tf("s_sc_gewaehlt", &[("0", Arg::from(size_of(6).as_str()))])), "{texts:?}");
    bench.click(&w.paths, &t("s_mlg_bestaetigen"));
    let done = tf("s_sc_fertig", &[("0", Arg::from(size_of(6).as_str()))]);
    let texts = bench.until(&w.paths, |texts| shows(texts, &done));
    assert!(shows(&texts, &done), "{texts:?}");
    assert!(Path::new(&w.current).is_file(), "switched off: stays");
    assert!(Path::new(&w.driver).is_file(), "switched off: stays");
    assert!(!Path::new(&w.old).exists() && !Path::new(&w.crash).exists());
    assert!(
        !shows(&texts, &name_current()) && !shows(&texts, &t("s_sc_ort_nvidia")),
        "no line for what stayed: {texts:?}"
    );
    // Everything off: "Bestätigen" is blocked, the question stays.
    let w2 = world("einzeln-leer", Some(7200), Some("sc-alpha-4.1"));
    let mut bench = Bench::new();
    ask(&mut bench, &w2.paths, "s_sc_knopf");
    for name in [name_current(), name_old(), t("s_sc_ort_nvidia"), t("s_sc_ort_crash")] {
        bench.switch(&w2.paths, &name);
    }
    bench.click(&w2.paths, &t("s_mlg_bestaetigen"));
    let texts = bench.settle(&w2.paths);
    assert!(shows(&texts, &t("s_mlg_bestaetigen")), "still asking: {texts:?}");
    assert!(!bench.state.phase_is_idle());
    for file in [&w2.current, &w2.old, &w2.driver, &w2.crash] {
        assert!(Path::new(file).is_file(), "{file}: nothing chosen, nothing deleted");
    }
    let _ = std::fs::remove_dir_all(w.home);
    let _ = std::fs::remove_dir_all(w2.home);
}

#[test]
fn only_old_lists_the_versions_offers_a_switch_only_at_the_old_one_and_keeps_the_installed_version() {
    let _language = setup();
    let w = world("nur-alte", Some(7200), Some("sc-alpha-4.1"));
    let mut bench = Bench::new();
    let texts = ask(&mut bench, &w.paths, "s_sc_knopf_alt");
    assert!(shows(&texts, &t("s_sc_alt_kopf")), "{texts:?}");
    assert!(shows(&texts, &name_old()) && shows(&texts, &name_current()), "{texts:?}");
    assert!(!shows(&texts, &t("s_sc_ort_nvidia")) && !shows(&texts, &t("s_sc_ort_crash")), "versions only: {texts:?}");
    assert!(
        shows(
            &texts,
            &tf(
                "s_sc_zeile",
                &[("0", Arg::from(size_of(3).as_str())), ("1", Arg::from(t("s_sc_bleibt_aktuell").as_str()))]
            )
        ),
        "the installed version says that it stays: {texts:?}"
    );
    assert!(shows(&texts, &tf("s_sc_gewaehlt", &[("0", Arg::from(size_of(5).as_str()))])), "{texts:?}");
    let switches: Vec<&str> =
        bench.nodes.iter().filter(|n| n.0 == egui::Role::CheckBox).map(|n| n.1.as_str()).collect();
    assert_eq!(switches, [name_old().as_str()], "a switch only where the version is old");
    bench.click(&w.paths, &t("s_mlg_bestaetigen"));
    let done = tf("s_sc_fertig", &[("0", Arg::from(size_of(5).as_str()))]);
    let texts = bench.until(&w.paths, |texts| shows(texts, &done));
    assert!(shows(&texts, &done), "{texts:?}");
    assert!(!Path::new(&w.old).exists(), "the old version is emptied");
    assert!(Path::new(&w.current).is_file(), "the installed version stays");
    assert!(Path::new(&w.driver).is_file() && Path::new(&w.crash).is_file(), "drivers and crash reports stay");
    let _ = std::fs::remove_dir_all(w.home);
}

#[test]
fn only_old_leaves_alone_what_became_the_installed_version_or_was_never_old_even_when_it_is_chosen() {
    let _language = setup();
    let w = world("nur-alte-fehler", Some(7200), Some("sc-alpha-4.1"));
    let mut bench = Bench::new();
    ask(&mut bench, &w.paths, "s_sc_knopf_alt");
    // A mistake in the page: every place switched on, the installed version as well.
    let Phase::Asking(asked) = &mut bench.state.phase else { panic!("not asking") };
    asked.chosen.iter_mut().for_each(|on| *on = true);
    bench.click(&w.paths, &t("s_mlg_bestaetigen"));
    let kept = tf("s_sc_erg_blieb", &[("0", Arg::from(name_current().as_str()))]);
    let texts = bench.until(&w.paths, |texts| shows(texts, &kept));
    assert!(shows(&texts, &kept), "the line says that the installed version was left alone: {texts:?}");
    assert!(Path::new(&w.current).is_file(), "the installed version is never touched");
    assert!(!Path::new(&w.old).exists());
    let _ = std::fs::remove_dir_all(w.home);
    // The installed version changes between the question and the yes (the folder asked as old is the installed
    // one now): the core asks again and leaves it.
    let w = world("nur-alte-patch", Some(7200), Some("sc-alpha-4.1"));
    let mut bench = Bench::new();
    ask(&mut bench, &w.paths, "s_sc_knopf_alt");
    let game = Path::new(&w.log).parent().expect("game folder").to_string_lossy().into_owned();
    std::fs::write(format!("{game}/build_manifest.id"), r#"{"Data": {"Branch": "sc-alpha-4.0"}}"#).expect("manifest");
    bench.click(&w.paths, &t("s_mlg_bestaetigen"));
    let kept = tf("s_sc_erg_blieb", &[("0", Arg::from(name_old().as_str()))]);
    let texts = bench.until(&w.paths, |texts| shows(texts, &kept));
    assert!(shows(&texts, &kept), "{texts:?}");
    assert!(Path::new(&w.old).is_file() && Path::new(&w.current).is_file(), "nothing was emptied");
    let _ = std::fs::remove_dir_all(w.home);
}

#[test]
fn only_old_says_so_and_asks_nothing_when_no_version_is_old_or_the_installed_one_cannot_be_told() {
    let _language = setup();
    // The installed version is the oldest one: nothing is older.
    for (name, branch, message) in
        [("keine-alte", Some("sc-alpha-4.0"), "s_sc_alt_keine"), ("unbekannt", None, "s_sc_alt_unklar")]
    {
        let w = world(name, Some(7200), branch);
        let mut bench = Bench::new();
        bench.click(&w.paths, &t("s_sc_knopf_alt"));
        let mut said = None;
        for _ in 0..60 {
            bench.settle(&w.paths);
            said = bench.state.take_notice().filter(|n| n.text == t(message) && !n.fault).or(said);
            if said.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(said.is_some(), "{name}: the footer says why nothing is cleared");
        assert!(bench.state.phase_is_idle(), "{name}");
        for file in [&w.current, &w.old, &w.driver, &w.crash] {
            assert!(Path::new(file).is_file(), "{name}: {file} stays");
        }
        let _ = std::fs::remove_dir_all(w.home);
    }
}

#[test]
fn nothing_is_deleted_while_the_game_runs_or_when_nobody_can_say_by_either_way() {
    let _language = setup();
    for button in ["s_sc_knopf", "s_sc_knopf_alt"] {
        for (name, age) in [("laeuft", Some(5)), ("unklar", None)] {
            let w = world(&format!("{button}-{name}"), age, Some("sc-alpha-4.1"));
            let mut bench = Bench::new();
            // The button is grey (the reason on hover): a click starts nothing, there is nothing to answer.
            bench.settle(&w.paths);
            assert!(!bench.enabled(egui::Role::Button, &t(button)), "{button} {name}: grey");
            bench.click(&w.paths, &t(button));
            let texts = bench.settle(&w.paths);
            assert!(!shows(&texts, &t("s_sc_sucht")) && !shows(&texts, &t("s_mlg_bestaetigen")), "{button} {name}");
            for file in [&w.current, &w.old, &w.driver, &w.crash] {
                assert!(Path::new(file).is_file(), "{button} {name}: {file} stays");
            }
            assert!(bench.state.phase_is_idle(), "{button} {name}");
            let _ = std::fs::remove_dir_all(w.home);
        }
        // Counter-check: the game quiet for two hours, the button does something.
        let w = world(&format!("{button}-ruhig"), Some(7200), Some("sc-alpha-4.1"));
        let mut bench = Bench::new();
        bench.settle(&w.paths);
        assert!(bench.enabled(egui::Role::Button, &t(button)), "{button}: not grey while the game is quiet");
        let _ = std::fs::remove_dir_all(w.home);
    }
}

#[test]
fn a_game_that_starts_while_the_question_stands_stops_the_deletion_at_the_yes_by_either_way() {
    let _language = setup();
    for button in ["s_sc_knopf", "s_sc_knopf_alt"] {
        let w = world(&format!("{button}-zweite-pruefung"), Some(7200), Some("sc-alpha-4.1"));
        let mut bench = Bench::new();
        ask(&mut bench, &w.paths, button);
        // The game starts now: its log is written.
        std::fs::File::options()
            .write(true)
            .open(&w.log)
            .and_then(|f| f.set_modified(std::time::SystemTime::now()))
            .expect("touch");
        bench.click(&w.paths, &t("s_mlg_bestaetigen"));
        let texts = bench.until(&w.paths, |texts| shows(texts, &t("s_sc_spiel")));
        assert!(shows(&texts, &t("s_sc_spiel")), "{button}: the window says why: {texts:?}");
        for file in [&w.current, &w.old, &w.driver, &w.crash] {
            assert!(Path::new(file).is_file(), "{button}: {file} was not deleted");
        }
        assert!(bench.state.phase_is_idle());
        let _ = std::fs::remove_dir_all(w.home);
    }
}

#[test]
fn an_empty_cache_says_so_and_asks_nothing() {
    let _language = setup();
    let w = world("leer", Some(7200), Some("sc-alpha-4.1"));
    for file in [&w.current, &w.old, &w.driver, &w.crash] {
        std::fs::remove_file(file).expect("remove");
    }
    let mut bench = Bench::new();
    bench.click(&w.paths, &t("s_sc_knopf"));
    let mut said = None;
    for _ in 0..60 {
        bench.settle(&w.paths);
        said = bench.state.take_notice().filter(|n| n.text == t("s_sc_leer") && !n.fault).or(said);
        if said.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(said.is_some(), "the footer says that there is nothing to empty");
    assert!(bench.state.phase_is_idle());
    let _ = std::fs::remove_dir_all(w.home);
}

/// A place of every kind with names far longer than any real one.
fn long_places() -> Vec<Place> {
    let long = "x".repeat(150);
    let place = |kind, branch: &str, suffix: &str, age| Place {
        kind,
        branch: branch.to_string(),
        suffix: suffix.to_string(),
        age,
        folder: String::new(),
        targets: Vec::new(),
        bytes: 15_990_000_000,
    };
    vec![
        place(PlaceKind::Game, "sc-alpha-4.10.2", &long, Age::Current),
        place(PlaceKind::Game, &format!("sc-alpha-{long}"), "cc9wt_0", Age::Old),
        place(PlaceKind::Game, "sc-alpha-4.9.0", "6kj0hb_0", Age::Unclear),
        place(PlaceKind::Nvidia, "", "", Age::Unclear),
        place(PlaceKind::Amd, "", "", Age::Unclear),
        place(PlaceKind::Intel, "", "", Age::Unclear),
        place(PlaceKind::DirectX, "", "", Age::Unclear),
        place(PlaceKind::Mesa, "", "", Age::Unclear),
        place(PlaceKind::OtherDriver, "", "", Age::Unclear),
        place(PlaceKind::Crash, "", "", Age::Unclear),
    ]
}

/// The question with a place of every kind and very long names, at the widths from far below the smallest window
/// to a wide one, in every scheme and both languages, for both ways: nothing sticks out to the right, and the two
/// ways to start and the first place are drawn.
#[test]
fn the_question_holds_long_names_at_every_width_scheme_and_language_and_nothing_sticks_out() {
    let _language = setup();
    let paths = world("breite", Some(7200), Some("sc-alpha-4.1"));
    let mut findings = Vec::new();
    let ctx = egui::Context::default();
    crate::install(&ctx);
    for scheme in theme::schemes() {
        theme::set_thread_current(scheme.id);
        for english in [false, true] {
            language::set_language(if english { Choice::En } else { Choice::De });
            for scope in [Scope::Everything, Scope::OnlyOld] {
                for width in [330.0, 480.0, 640.0, 900.0, 1400.0] {
                    let places = long_places();
                    let chosen = vec![true; places.len()];
                    let mut state =
                        ShaderState { phase: Phase::Asking(Asked { scope, places, chosen }), ..Default::default() };
                    let mut overflow = 0.0;
                    let mut texts = Vec::new();
                    for _ in 0..4 {
                        let input = egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 380.0))),
                            ..Default::default()
                        };
                        let mut output = ctx.run_ui(input, |ui| {
                            overflow = layout::page(ui, scheme, "shader", |ui| {
                                show_page(ui, scheme, &mut state, &paths.paths)
                            })
                            .overflow;
                        });
                        output.textures_delta.clear();
                        texts.clear();
                        fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
                            match shape {
                                egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
                                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
                                _ => {}
                            }
                        }
                        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
                    }
                    let id = format!("{} englisch {english} {scope:?} Breite {width}", scheme.id);
                    if overflow > 0.5 {
                        findings.push(format!("{id}: {overflow:.0} zu breit"));
                    }
                    for (key, kind) in [("s_sc_knopf_alt", ButtonKind::Strong), ("s_sc_knopf", ButtonKind::Danger)] {
                        let label = widgets::button_label(scheme, kind, &t(key));
                        if !texts.contains(&label) {
                            findings.push(format!("{id}: {key} is not drawn"));
                        }
                    }
                    if !texts.iter().any(|text| text.contains("4.10.2")) {
                        findings.push(format!("{id}: the first place is not drawn"));
                    }
                }
            }
        }
    }
    theme::set_current("original");
    language::set_language(Choice::De);
    let _ = std::fs::remove_dir_all(paths.home);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

/// The two ways to start stand in a grid of equal cells: side by side the same size at the same height, under each
/// other the same width at the same left edge.
#[test]
fn the_two_ways_to_start_stand_in_equal_cells_side_by_side_and_under_each_other() {
    let _language = setup();
    let w = world("raster", Some(7200), Some("sc-alpha-4.1"));
    for (width, side_by_side) in [(1000.0, true), (360.0, false)] {
        let mut bench = Bench::new();
        bench.size = egui::vec2(width, 700.0);
        bench.settle(&w.paths);
        let a = bench.place(&w.paths, egui::Role::Button, &t("s_sc_knopf_alt"));
        let b = bench.place(&w.paths, egui::Role::Button, &t("s_sc_knopf"));
        assert!((a.width() - b.width()).abs() < 0.5 && (a.height() - b.height()).abs() < 0.5, "{width}: {a:?} {b:?}");
        if side_by_side {
            assert!((a.top() - b.top()).abs() < 0.5 && a.right() < b.left(), "{width}: {a:?} {b:?}");
        } else {
            assert!((a.left() - b.left()).abs() < 0.5 && a.bottom() < b.top(), "{width}: {a:?} {b:?}");
        }
    }
    let _ = std::fs::remove_dir_all(w.home);
}
