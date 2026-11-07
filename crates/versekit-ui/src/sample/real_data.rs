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

//! Checks that real data show the real state and no content of the sample
//! page: the Basetool, the settings of "Allgemein" and the folders.
//!
//! Every test holds the language lock: it compares shown texts with the
//! language file, and the language is one for the whole process.

use egui::Pos2;
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::{font_size, language, theme};

use super::SamplePage;
use super::docking::tests::draw;

/// The host of a throwaway folder: the Basetool cannot go out, the home folder is the throwaway
/// folder too (nothing of the home folder of this computer is found).
pub(super) fn host_in(dir: &std::path::Path, vars: &[(&str, &str)]) -> Host {
    use versekit_core::legacy::{ENV_BASETOOL_KEYNAME, ENV_HOME, ENV_NO_NET};
    let home = dir.to_string_lossy().into_owned();
    let mut all = vec![
        (ENV_HOME, home.as_str()),
        ("HOME", home.as_str()),
        ("USERPROFILE", home.as_str()),
        (ENV_NO_NET, "1"),
        (ENV_BASETOOL_KEYNAME, "VerseKit Pruefung"),
    ];
    all.extend_from_slice(vars);
    Host::new(Flavor::native(), &all, &home)
}

/// Like [`host_in`], with a game folder of its own (an empty `Game.log` in `LIVE` of the
/// throwaway folder): where the program looks for the game on the drives of the computer, a test
/// does not find the installation that happens to be there, neither its logs nor its `Data.p4k`.
pub(super) fn host_apart(dir: &std::path::Path) -> Host {
    let game = dir.join("LIVE");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join("Game.log"), "").unwrap();
    let game = game.to_string_lossy().into_owned();
    host_in(dir, &[(versekit_core::paths::ENV_INSTALL_DIR, game.as_str())])
}

/// Whether the program finds a game installation on this computer with no setting at all.
pub(super) fn game_on_this_computer() -> bool {
    let dir = folder("rechner");
    let found = Paths::new(host_in(&dir, &[])).game_folder().is_some();
    let _ = std::fs::remove_dir_all(dir);
    found
}

/// A throwaway folder of this test, empty.
pub(super) fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("versekit-echt-{name}-{}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A page on real data in a throwaway folder. The caller holds the language lock.
/// `settings` is the content of the settings file; `vars` more variables of the host.
pub(super) fn real_page(name: &str, settings: &str, vars: &[(&str, &str)]) -> (SamplePage, std::path::PathBuf) {
    let dir = folder(name);
    if !settings.is_empty() {
        std::fs::write(dir.join("einstellungen.json"), settings).unwrap();
    }
    theme::set_current("original");
    language::set_language(language::Choice::De);
    (SamplePage::with_settings(Paths::new(host_in(&dir, vars))), dir)
}

fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
    texts.iter().any(|(text, _)| text.contains(part))
}

/// Draws the page again and again until every text of `needed` stands in it, for at most a minute (the reads of
/// the page run in threads of their own; on a busy processor they take as long as they take). Whether it came.
fn wait_until_drawn(page: &mut SamplePage, width: f32, height: f32, needed: &[String]) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    loop {
        let texts = draw(page, width, height).1;
        if needed.iter().all(|text| shows(&texts, text)) {
            return true;
        }
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

fn look(page: &mut SamplePage, key: &str) -> Vec<(String, Pos2)> {
    let (width, height) = font_size::BASE_MIN_SIZE;
    page.go_to(key);
    draw(page, width.max(1280.0), height.max(820.0)).1
}

#[test]
fn real_data_connect_to_the_real_basetool_and_not_to_the_made_up_one() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("basetool", "", &[]);
    let wire = page.connection.wire().expect("the connection of the program");
    assert!(wire.demo.is_none(), "no made-up Basetool on real data");
    assert!(!wire.sync.linked(), "nobody connected");
    let texts = look(&mut page, "k_basetool_verbindung");
    assert!(shows(&texts, &language::t("s_btv_nicht_verbunden")), "the real state: not connected");
    assert!(!shows(&texts, &language::t("s_btv_beispiel")), "no made-up Basetool");
    assert!(!shows(&texts, "PlayerOne"));
    // The storage of the unit comes with a connection, switch and right only: the page is not there.
    assert!(page.page_hidden("hf_staffellager"));

    // Counter-check: the sample page keeps its made-up Basetool.
    let mut sample = SamplePage::preview();
    assert!(sample.connection.wire().is_some_and(|wire| wire.demo.is_some()));
    let texts = look(&mut sample, "k_basetool_verbindung");
    assert!(shows(&texts, &language::t("s_btv_beispiel")), "the sample page says it is made up");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_start_programs_have_an_entry_of_their_own_in_the_settings_group_and_open_on_real_data() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("startprogramme", "", &[]);
    let entry = language::t("hf_startprogramme");
    // A tall window: the menu is a list that scrolls.
    let tall = |page: &mut SamplePage, key: &str| {
        page.go_to(key);
        draw(page, 1280.0, 2600.0).1
    };
    let texts = tall(&mut page, "hf_ordner");
    assert!(shows(&texts, &entry), "the entry is in the menu, no folded block in front of it: {texts:?}");
    assert!(
        !shows(&texts, &language::t("hf_fortgeschritten").to_uppercase()),
        "and there is no group For advanced users (decision 21.10.): {texts:?}"
    );
    // The page opens and shows its own words, not the placeholder.
    let texts = tall(&mut page, "hf_startprogramme");
    assert!(shows(&texts, &language::t("s_pg_lead")), "{texts:?}");
    assert!(shows(&texts, &language::t("s_pg_risiko")), "{texts:?}");
    assert!(!shows(&texts, &language::t("s_mu_seite")), "no placeholder page");
    let _ = std::fs::remove_dir_all(dir);
}

/// A throwaway world for the page Grafik: a game folder with the three files and a Game.log in which the
/// computer has names of `long` signs. Gives the folder and the two paths the host needs.
fn graphics_world(name: &str, long: &str) -> (std::path::PathBuf, String, String) {
    // Not the folder of the page itself: `real_page` makes its folder anew.
    let dir = folder(name);
    let game = dir.join("LIVE");
    let profile = game.join("USER").join("Client").join("0").join("Profiles").join("default");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("attributes.xml"),
        "<Attributes>\n <Attr name=\"SysSpec\" value=\"5\"/>\n</Attributes>\n",
    )
    .unwrap();
    std::fs::write(game.join("user.cfg"), "sys_MaxFPS = 60\n").unwrap();
    std::fs::write(
        game.join("Game.log"),
        format!(
            "Host CPU: {long}\nLogical CPU Count: 16\n32768MB physical memory installed\n[VK] Chosen Vulkan GPU Device ({long}) Driver Version ({long}) Vulkan API (1.3)\nCurrent display mode is 2560x1440\nPerformance Index: 8.5 (CPU), 12.75 (GPU)\nFinal rating: Machine class 2\n"
        ),
    )
    .unwrap();
    let local = dir.join("AppData").join("Local");
    let json = local.join("Star Citizen").join("starcitizen_(sc-alpha-4.2)_x").join("GraphicsSettings");
    std::fs::create_dir_all(&json).unwrap();
    std::fs::write(json.join("GraphicsSettings.json"), "{\"GraphicsSettings\": {\"GraphicsRenderer\": 1}}").unwrap();
    (dir, game.to_string_lossy().into_owned(), local.to_string_lossy().into_owned())
}

/// The variables of the host for [`graphics_world`].
fn graphics_vars<'a>(game: &'a str, local: &'a str) -> [(&'a str, &'a str); 3] {
    [
        (versekit_core::paths::ENV_INSTALL_DIR, game),
        ("LOCALAPPDATA", local),
        (versekit_core::game_start::ENV_NO_PROCESS, "1"),
    ]
}

/// The bar of the graphics page stands above the area that scrolls (4.12): scrolled to the end, the button is
/// still in sight and the end of the page is in sight too; at the start the top of the page is. Counter-check:
/// a bar that scrolls away with the page (drawn inside the area) is gone at the end.
#[test]
fn the_bar_of_the_graphics_page_stays_in_sight_when_the_page_is_scrolled_to_the_end() {
    let _language = crate::language_lock();
    let (dir, game, local) = graphics_world("grafik-rollen", "x");
    let (mut page, _) = real_page("grafik-rollen-seite", "", &graphics_vars(&game, &local));
    page.go_to("hf_grafik");
    let (width, height) = font_size::min_window(font_size::POINTS_RANGE.0);
    // A window high enough for the bar and not for the page.
    let (end, start) = super::docking::tests::draw_scrolled_to_end(&mut page, width, height);
    let button = language::t("s_gr_schreiben");
    // The page ends with the card of the shader cache (since 5.1.4).
    let last_row = language::t("s_sc_knopf_h");
    assert!(shows(&start, &button) && shows(&start, &language::t("s_gr_lead")), "at the start: {start:?}");
    assert!(!shows(&start, &last_row), "the last row is below the window at the start: {start:?}");
    assert!(shows(&end, &last_row), "scrolled to the end the last row is in sight: {end:?}");
    assert!(shows(&end, &button), "and the button with it: {end:?}");
    assert!(shows(&end, &language::t("s_gr_alles_drin")), "and what waits: {end:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// The page Blickwinkel with a measured screen, a distance and the file of the game: at every size, scheme and
/// language nothing sticks out to the right, and the real page is drawn.
#[test]
fn the_view_angle_page_holds_its_texts_at_every_size_scheme_and_language() {
    let _language = crate::language_lock();
    let (dir, game, local) = graphics_world("blickwinkel-welt", "x");
    let profile = std::path::Path::new(&game).join("USER").join("Client").join("0").join("Profiles").join("default");
    std::fs::write(
        profile.join("attributes.xml"),
        "<Attributes>\n <Attr name=\"FOV\" value=\"54.7681\"/>\n <Attr name=\"Width\" value=\"5120\"/>\n <Attr name=\"Height\" value=\"1440\"/>\n</Attributes>\n",
    )
    .unwrap();
    let settings = r#"{"fov_mm_je_pixel": 0.233, "fov_pixelbreite": 5120, "fov_abstand_mm": 700}"#;
    let (mut page, _) = real_page("blickwinkel-lang", settings, &graphics_vars(&game, &local));
    let mut findings = Vec::new();
    for scheme in theme::schemes() {
        theme::set_thread_current(scheme.id);
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            page.size = points;
            let (width, height) = font_size::min_window(points);
            for english in [false, true] {
                language::set_language(if english { language::Choice::En } else { language::Choice::De });
                page.go_to("hf_blickwinkel");
                let mut shown = (0.0, Vec::new());
                for _ in 0..4 {
                    shown = draw(&mut page, width, height);
                }
                let (overflow, texts) = shown;
                if overflow > 0.5 {
                    findings.push(format!("{} Größe {points} englisch {english}: {overflow:.0} zu breit", scheme.id));
                }
                assert!(shows(&texts, &language::t("s_fv_lead")), "{} {points} {english}: {texts:?}", scheme.id);
                assert!(
                    shows(&texts, &language::t("s_fv_neu_messen")),
                    "the measured screen: {} {points} {english}",
                    scheme.id
                );
            }
        }
    }
    theme::set_current("original");
    language::set_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

/// The menu filter finds what the real page Grafik shows (VSync, the frame rate, the computer) and not the one
/// made-up row of the sample page; the sample page keeps its row.
#[test]
fn the_menu_filter_finds_the_rows_of_the_real_graphics_page_and_not_the_made_up_one() {
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let hits = |filter: &str, sample: bool| super::nav_hits_in(filter, "hf_gruppe_spiel", "hf_grafik", sample);
    let page = language::t("hf_grafik");
    assert!(
        hits("vsync", false).contains(&format!("{page} › {}", language::t("s_gr_vsync"))),
        "{:?}",
        hits("vsync", false)
    );
    assert!(hits("bildrate", false).len() >= 2, "both frame rates: {:?}", hits("bildrate", false));
    assert!(
        hits("prozessor", false).iter().any(|h| h.contains(&language::t("s_gr_cpu"))),
        "the box: {:?}",
        hits("prozessor", false)
    );
    assert!(hits("ultra", false).iter().any(|h| h.contains(&language::t("s_gr_q_gesamt"))), "a level as a choice");
    let made_up = format!("{page} › {}", language::t("s_ei_sichtweite"));
    assert!(
        !hits("sichtweite", false).contains(&made_up),
        "no made-up row on real data: {:?}",
        hits("sichtweite", false)
    );
    assert!(hits("sichtweite", true).contains(&made_up), "the sample page keeps it: {:?}", hits("sichtweite", true));
    assert!(
        !hits("vsync", true).iter().any(|h| h.contains("VSync")),
        "and nothing of the real page: {:?}",
        hits("vsync", true)
    );
}

/// The page Grafik with the game's files and a computer with names far longer than any real one: at every size,
/// scheme and language nothing sticks out to the right, and the page is the real one, not the sample.
#[test]
fn the_graphics_page_holds_long_texts_at_every_size_scheme_and_language() {
    let _language = crate::language_lock();
    let long = "x".repeat(150);
    let (dir, game_path, local_path) = graphics_world("grafik-welt", &long);
    let vars = graphics_vars(&game_path, &local_path);
    let (mut page, _) = real_page("grafik-lang", "", &vars);
    let mut findings = Vec::new();
    for scheme in theme::schemes() {
        theme::set_thread_current(scheme.id);
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            page.size = points;
            let (width, height) = font_size::min_window(points);
            for english in [false, true] {
                language::set_language(if english { language::Choice::En } else { language::Choice::De });
                page.go_to("hf_grafik");
                // The box reads the log in a thread of its own: frames until it shows.
                let mut shown = (0.0, Vec::new());
                for _ in 0..40 {
                    shown = draw(&mut page, width, height);
                    if shows(&shown.1, &language::t("s_gr_cpu")) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                // A few frames more: the first one with the box in it is not yet at rest.
                for _ in 0..3 {
                    shown = draw(&mut page, width, height);
                }
                let (overflow, texts) = shown;
                if overflow > 0.5 {
                    findings.push(format!("{} Größe {points} englisch {english}: {overflow:.0} zu breit", scheme.id));
                }
                assert!(shows(&texts, &language::t("s_gr_lead")), "{} {points} {english}: {texts:?}", scheme.id);
                // What lies below the window is not drawn: the box stands at the top, the rows of the files after it.
                assert!(
                    shows(&texts, &language::t("s_gr_cpu")),
                    "the box: {} {points} {english}: {texts:?}",
                    scheme.id
                );
                assert!(shows(&texts, "Vulkan"), "the rows: {} {points} {english}", scheme.id);
            }
        }
    }
    theme::set_current("original");
    language::set_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

/// The page Steuerung with sticks whose names are far longer than any real one, in the log, the binding and the
/// system, in the two cases with advice (a swap; two missing): at every size, scheme and language, at the
/// smallest and at a very wide window, nothing sticks out to the right, the long name, the states and the advice
/// are drawn, and the page is the real one.
#[test]
fn the_controls_page_holds_long_names_at_every_size_scheme_and_language() {
    use versekit_core::input_device::SystemDevice;
    let _language = crate::language_lock();
    let long = "x".repeat(150);
    let action = format!("a{}", "y".repeat(150));
    let (left, right, fresh, pedals) = (
        "1234ABCD-0000-0000-0000-504944564944",
        "3344ABCD-0000-0000-0000-504944564944",
        "99990000-0000-0000-0000-504944564944",
        "55550000-0000-0000-0000-504944564944",
    );
    let mut findings = Vec::new();
    // (name, a third device only in the binding?, the advice that must be drawn)
    for (name, with_pedals, advice) in [("tausch", false, "s_gh_tausch"), ("zwei-fehlen", true, "s_gh_anstecken_rat")] {
        let dir = folder(&format!("steuerung-welt-{name}"));
        let game = dir.join("LIVE");
        let profile = game.join("USER").join("Client").join("0").join("Profiles").join("default");
        std::fs::create_dir_all(&profile).unwrap();
        std::fs::write(
            game.join("Game.log"),
            format!("Connected joystick0:  {long} L  {{{left}}}\nConnected joystick1:  {long} R  {{{right}}}\n"),
        )
        .unwrap();
        let third = if with_pedals {
            format!("<options type=\"joystick\" instance=\"3\" Product=\" {long} P  {{{pedals}}}\"/>")
        } else {
            String::new()
        };
        std::fs::write(
            profile.join("actionmaps.xml"),
            format!(
                "<ActionMaps><ActionProfiles><options type=\"joystick\" instance=\"1\" Product=\" {long} L  {{{left}}}\"/><options type=\"joystick\" instance=\"2\" Product=\" {long} R  {{{right}}}\"/>{third}<actionmap name=\"spaceship_general\"><action name=\"{action}\"><rebind input=\"js1_button1\"/><rebind input=\"kb1_lalt+ralt+np_add\"/></action></actionmap></ActionProfiles></ActionMaps>"
            ),
        )
        .unwrap();
        let game = game.to_string_lossy().into_owned();
        let vars = [(versekit_core::paths::ENV_INSTALL_DIR, game.as_str())];
        let (mut page, page_dir) = real_page(&format!("steuerung-lang-{name}"), "", &vars);
        let system = vec![
            SystemDevice { path: format!("\\\\?\\HID#{long}"), name: format!("{long} L"), id: left.into() },
            SystemDevice { path: "/dev/input/js1".into(), name: format!("{long} N"), id: fresh.into() },
        ];
        let list = system.clone();
        page.joysticks = crate::joysticks_page::JoystickState::with_source(move || list.clone());
        page.go_to("hf_joysticks");
        // The look is made in a thread of its own: frames until the list shows (the texts are put in at every
        // draw, so one look serves all sizes, schemes and languages).
        // A tall window: what lies below the window is not drawn, and the whole page is to be checked.
        let tall = 4000.0;
        let (width, _) = font_size::min_window(font_size::POINTS_RANGE.0);
        let height = tall;
        // Both reads (the devices, then the names from the game) must be through before the sizes are walked:
        // the long action and the state stand in the page only after them, and a processor that is busy with
        // other tests lets them take their time.
        let needed = [language::t("s_js_geraete"), action.clone(), language::t("s_gh_bereit")];
        let loaded = wait_until_drawn(&mut page, width, height, &needed);
        assert!(loaded, "the look never came: the test would pass with the wheel on the screen");
        for scheme in theme::schemes() {
            theme::set_thread_current(scheme.id);
            for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
                page.size = points;
                for wide in [false, true] {
                    let (width, _) = font_size::min_window(points);
                    let (width, height) = if wide { (1800.0, tall) } else { (width, tall) };
                    for english in [false, true] {
                        language::set_language(if english { language::Choice::En } else { language::Choice::De });
                        page.go_to("hf_joysticks");
                        let mut shown = (0.0, Vec::new());
                        for _ in 0..3 {
                            shown = draw(&mut page, width, height);
                        }
                        let (overflow, texts) = shown;
                        let which = format!("{name} {} Größe {points} breit {wide} englisch {english}", scheme.id);
                        if overflow > 0.5 {
                            findings.push(format!("{which}: {overflow:.0} zu breit"));
                        }
                        // What the page must draw: its lines, the long name in them, the states, the advice.
                        let advice_head = language::t(advice).split('{').next().unwrap_or_default().to_string();
                        for (what, text) in [
                            ("lead", language::t("s_js_lead")),
                            ("hub", language::t("s_gh_titel")),
                            ("list", language::t("s_js_geraete")),
                            ("long name", long.clone()),
                            ("list of the bindings", language::t("s_js_belegt")),
                            ("long action", action.clone()),
                            ("long input", language::t("s_js_t_lalt")),
                            ("state", language::t("s_gh_bereit")),
                            ("advice", advice_head),
                        ] {
                            assert!(shows(&texts, &text), "{which}: {what} is not drawn: {texts:?}");
                        }
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(dir);
        let _ = std::fs::remove_dir_all(page_dir);
    }
    theme::set_current("original");
    language::set_language(language::Choice::De);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

/// The page Achsen & Kurven in every size, scheme and language, at the smallest window of the size and at a very
/// wide one, with names far longer than any real one: nothing sticks out to the right, and the names, the axes,
/// the picture, the sliders and the sensitivity are drawn.
#[test]
fn the_axes_page_holds_long_names_at_every_size_scheme_and_language() {
    let _language = crate::language_lock();
    let long = "x".repeat(150);
    let (left, right) = ("1234ABCD-0000-0000-0000-504944564944", "3344ABCD-0000-0000-0000-504944564944");
    let dir = folder("achsen-welt");
    let game = dir.join("LIVE");
    let profile = game.join("USER").join("Client").join("0").join("Profiles").join("default");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        game.join("Game.log"),
        format!("Connected joystick0:  {long} L  {{{left}}}\nConnected joystick1:  {long} R  {{{right}}}\n"),
    )
    .unwrap();
    std::fs::write(
        profile.join("actionmaps.xml"),
        format!(
            "<ActionMaps><ActionProfiles><deviceoptions name=\"{long} L  {{{left}}}\"><option input=\"x\" deadzone=\"0.15\"/><option input=\"x\" saturation=\"0.85\"/><option input=\"y\" deadzone=\"0.1\"/><option input=\"slider1\" saturation=\"0.9\"/></deviceoptions><deviceoptions name=\"{long} R  {{{right}}}\"><option input=\"x\" deadzone=\"0.2\"/></deviceoptions><deviceoptions name=\"{long} L  {{99990000-0000-0000-0000-504944564944}}\"><option input=\"x\" deadzone=\"0.3\"/><option input=\"y\" saturation=\"0.6\"/></deviceoptions><options type=\"joystick\" instance=\"1\" Product=\"{long} L  {{{left}}}\"><flight_move_pitch exponent=\"1.5\"/></options><options type=\"joystick\" instance=\"2\" Product=\"{long} R  {{{right}}}\"/><actionmap name=\"spaceship_general\"><action name=\"v_pitch\"><rebind input=\"js1_x\"/></action><action name=\"v_roll\"><rebind input=\"js1_x\"/></action><action name=\"v_{long}\"><rebind input=\"js1_x\"/></action></actionmap></ActionProfiles></ActionMaps>"
        ),
    )
    .unwrap();
    let game = game.to_string_lossy().into_owned();
    let vars = [(versekit_core::paths::ENV_INSTALL_DIR, game.as_str())];
    let (mut page, page_dir) = real_page("achsen-lang", "", &vars);
    // A set with the longest name there is, so that a row of the sets is on the page.
    let set = "y".repeat(versekit_core::device_sets::NAME_MAX);
    let paths = Paths::new(host_in(&page_dir, &vars));
    versekit_core::device_sets::save(&paths, &set, false, None, None).expect("the set is saved");
    page.axes.open_leftovers();
    page.go_to("hf_achsen");
    let tall = 4000.0;
    let (width, _) = font_size::min_window(font_size::POINTS_RANGE.0);
    // The devices are read in a thread of their own: the page is looked at again until the dead zone, the long
    // name and the axis stand in it (what the sizes are walked for), not for a time.
    let needed = [language::t("s_kv_totzone"), long.clone(), "slider1".to_string()];
    let loaded = wait_until_drawn(&mut page, width, tall, &needed);
    assert!(loaded, "the look never came: the test would pass with the wheel on the screen");
    let mut findings = Vec::new();
    for scheme in theme::schemes() {
        theme::set_thread_current(scheme.id);
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            page.size = points;
            for wide in [false, true] {
                let (width, _) = font_size::min_window(points);
                let (width, height) = if wide { (1800.0, tall) } else { (width, tall) };
                for english in [false, true] {
                    language::set_language(if english { language::Choice::En } else { language::Choice::De });
                    page.go_to("hf_achsen");
                    let mut shown = (0.0, Vec::new());
                    for _ in 0..3 {
                        shown = draw(&mut page, width, height);
                    }
                    let (overflow, texts) = shown;
                    let which = format!("{} Größe {points} breit {wide} englisch {english}", scheme.id);
                    if overflow > 0.5 {
                        findings.push(format!("{which}: {overflow:.0} zu breit"));
                    }
                    for (what, text) in [
                        ("lead", language::t("s_ac_lead")),
                        ("long name", long.clone()),
                        ("the dead zone", language::t("s_kv_totzone")),
                        ("the saturation", language::t("s_kv_saettigung")),
                        ("the view", language::t("s_kv_ganz")),
                        ("the large view", language::t("s_ac_gross")),
                        ("sensitivity", language::t("s_ac_kopf_empf")),
                        ("the presets", language::t("s_ac_kopf_vor")),
                        ("a preset", language::t("s_ac_vor_fine")),
                        (
                            "whose sensitivity the curve shows",
                            language::t("s_ac_zeigt").split("{0}").next().unwrap_or_default().to_string(),
                        ),
                        ("the choice of the function", language::t("s_ac_zeigt_mehrere")),
                        // The ticks and the sets stand in menus behind these buttons (each tested on the page).
                        ("copying to other axes", language::t("s_ac_kopf_uebertragen")),
                        ("the menu of the device", language::t("s_ac_menue_geraet")),
                        ("the menu of the sets", language::t("s_ac_menue_satz")),
                        ("the axis", "slider1".to_string()),
                        ("the old entries", language::t("s_ac_befund")),
                        ("the clearing", language::t("s_ac_aufraeumen")),
                        ("the carrying over", language::t("s_ac_uebernehmen")),
                        ("writing", language::t("s_ac_speichern")),
                    ] {
                        // A button of the scheme KRT is in capitals.
                        let drawn = shows(&texts, &text) || shows(&texts, &text.to_uppercase());
                        assert!(drawn, "{which}: {what} is not drawn: {texts:?}");
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(page_dir);
    theme::set_current("original");
    language::set_language(language::Choice::De);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

/// The menu filter finds what the real page Steuerung shows.
#[test]
fn the_menu_filter_finds_the_blocks_of_the_real_controls_page() {
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let hits = |filter: &str| super::nav_hits_in(filter, "hf_gruppe_spiel", "hf_joysticks", false);
    let page = language::t("hf_joysticks");
    assert!(
        hits("deine geräte").contains(&format!("{page} › {}", language::t("s_gh_titel"))),
        "{:?}",
        hits("deine geräte")
    );
    assert!(hits("was zu tun").iter().any(|h| h.contains(&language::t("s_gh_was_tun"))), "{:?}", hits("was zu tun"));
    // The list of the bindings and the buttons above it are found as well.
    for key in ["s_js_belegt", "s_js_nur_eigene", "s_js_zurueck", "s_js_profil", "s_js_ausgeben_csv", "s_js_einlesen"] {
        let text = language::t(key).to_lowercase();
        assert!(hits(&text).iter().any(|h| h.starts_with(&page)), "{key}: {:?}", hits(&text));
    }
}

/// Every size, every scheme, both languages, the smallest window of the size: nothing of the pages of the
/// game settings sticks out to the right, with names, paths and arguments far longer than any real one.
#[test]
fn the_pages_of_the_game_settings_hold_long_texts_at_every_size_scheme_and_language() {
    let _language = crate::language_lock();
    let long = "x".repeat(150);
    let settings = format!(
        r#"{{"startprogramme": [{{"an": true, "name": "{long}", "datei": "C:/{long}/{long}.exe", "argumente": "{long}", "wann": "ersetzt", "warten": 5, "beenden": true}}, {{"name": "B"}}]}}"#
    );
    let (mut page, dir) = real_page("spielseiten-lang", &settings, &[]);
    let mut findings = Vec::new();
    for scheme in theme::schemes() {
        theme::set_thread_current(scheme.id);
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            page.size = points;
            let (width, height) = font_size::min_window(points);
            for english in [false, true] {
                language::set_language(if english { language::Choice::En } else { language::Choice::De });
                for key in ["hf_startprogramme", "hf_module", "hf_erkennung", "hf_grafik"] {
                    page.go_to(key);
                    let (overflow, texts) = draw(&mut page, width, height);
                    if overflow > 0.5 {
                        findings.push(format!(
                            "{key} {} Größe {points} englisch {english}: {overflow:.0} zu breit",
                            scheme.id
                        ));
                    }
                    // The page is drawn (not the placeholder of a page that is not built): its own first line.
                    let lead = match key {
                        "hf_startprogramme" => "s_pg_lead",
                        "hf_erkennung" => "s_er_lead",
                        "hf_grafik" => "s_gr_lead",
                        _ => "s_mo_lead",
                    };
                    assert!(shows(&texts, &language::t(lead)), "{key} {} {points} {english}: {texts:?}", scheme.id);
                }
            }
        }
    }
    theme::set_current("original");
    language::set_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

/// The shader cache is the last card of the graphics page since 5.1.4: the old page (a stored name, a pinned card, a
/// link) opens Grafik with the card framed, the menu filter finds the card, and the side bar has no row of its own.
#[test]
fn the_old_shader_page_and_the_filter_lead_to_the_card_on_the_graphics_page() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("shader-karte", "", &[]);
    let needed = [language::t("s_sc_titel"), language::t("s_sc_knopf")];
    page.go_to("hf_shader");
    assert!(wait_until_drawn(&mut page, 1280.0, 4000.0, &needed), "the card stands on the page");
    assert_eq!(page.page_key(), "hf_grafik");
    assert_eq!(page.highlight.as_ref().map(|(text, _)| text.clone()), Some(language::t("s_sc_titel")), "framed");
    // Counter-check: Grafik opened on its own frames nothing, and still has the card.
    page.highlight = None;
    page.go_to("hf_allgemein");
    draw(&mut page, 1280.0, 4000.0);
    page.go_to("hf_grafik");
    assert!(wait_until_drawn(&mut page, 1280.0, 4000.0, &needed));
    assert!(page.highlight.is_none());
    assert!(!super::NAV.iter().flat_map(|(_, pages)| pages.iter()).any(|(key, _)| *key == "hf_shader"));
    // The menu filter finds the card by its name, by a button and by the old name of the page.
    let hit = format!("{} › {}", language::t("hf_grafik"), language::t("s_sc_titel"));
    for word in [language::t("s_sc_titel"), language::t("s_sc_knopf"), "Shader".to_string()] {
        let hits = super::nav_hits_in(&word, "hf_gruppe_spiel", "hf_grafik", false);
        assert!(hits.contains(&hit), "{word}: {hits:?}");
    }
    assert!(super::nav_hits_in("zzz", "hf_gruppe_spiel", "hf_grafik", false).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

/// The block Signatur-Fenster of the page Overlay with its two models in a card each: at every size, scheme and
/// language, at the smallest window of the size, nothing sticks out and both cards are drawn with their titles.
#[test]
fn the_two_models_of_the_signature_window_hold_at_every_size_scheme_and_language() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("signatur-modelle", "{}", &[]);
    let mut findings = Vec::new();
    for scheme in theme::schemes() {
        theme::set_thread_current(scheme.id);
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            page.size = points;
            let (min_width, min_height) = font_size::min_window(points);
            for width in [min_width, min_width * 2.0] {
                for english in [false, true] {
                    language::set_language(if english { language::Choice::En } else { language::Choice::De });
                    page.go_to("hf_overlay");
                    let (overflow, texts) = draw(&mut page, width, min_height);
                    if overflow > 0.5 {
                        findings
                            .push(format!("{} Größe {points} englisch {english}: {overflow:.0} zu breit", scheme.id));
                    }
                    // Labels below the fold are not painted; the models paint their caption wherever they stand.
                    let captions = texts.iter().filter(|text| text.0 == language::t("s_sg_modell_spiel")).count();
                    if captions != 2 {
                        findings.push(format!("{captions} models: {} Größe {points} englisch {english}", scheme.id));
                    }
                }
            }
        }
    }
    theme::set_current("original");
    language::set_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

/// The block user.cfg and the block Anti-AFK on the page Spiel and the bubbles of the channels on the page
/// Installation & Pfade, with a user.cfg whose lines are far longer than any real one, two channels, in every size,
/// scheme and language, at the smallest window of the size and at a wide one: nothing sticks out to the right, and
/// the titles, the switch, the buttons and the bubbles are drawn.
#[test]
fn the_blocks_of_user_cfg_and_anti_afk_and_the_bubbles_of_the_channels_hold_long_texts_everywhere() {
    use versekit_core::usercfg::Status;
    let _language = crate::language_lock();
    let long = "x".repeat(150);
    let dir = folder("kanaele-welt");
    let root = dir.join("StarCitizen");
    for (channel, cfg) in [
        ("LIVE", format!("g_language = german_(germany)\n{long} = {long}\nsys_MaxFPS = 60\n; {long}\n")),
        ("PTU", "sys_MaxFPS = 30\n".to_string()),
    ] {
        let folder = root.join(channel);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("Data.p4k"), "").unwrap();
        std::fs::write(folder.join("Game.log"), "").unwrap();
        std::fs::write(folder.join("user.cfg"), cfg).unwrap();
    }
    let live = root.join("LIVE").to_string_lossy().into_owned();
    let vars = [(versekit_core::paths::ENV_INSTALL_DIR, live.as_str())];
    let (mut page, page_dir) = real_page("kanaele-lang", "", &vars);
    let paths = &page.store.as_ref().expect("real data").paths;
    // The states the bubbles must show.
    let live_folder = root.join("LIVE").to_string_lossy().into_owned();
    assert_eq!(versekit_core::usercfg::status(&live_folder), Status::Ok);
    assert_eq!(paths.installed_channels().len(), 2, "two channels are found");
    let mut findings = Vec::new();
    for scheme in theme::schemes() {
        theme::set_thread_current(scheme.id);
        for points in font_size::POINTS_RANGE.0..=font_size::POINTS_RANGE.1 {
            page.size = points;
            for wide in [false, true] {
                let (width, _) = font_size::min_window(points);
                let (width, height) = if wide { (1800.0, 4000.0) } else { (width, 4000.0) };
                for english in [false, true] {
                    language::set_language(if english { language::Choice::En } else { language::Choice::De });
                    for (key, wanted) in [
                        (
                            "hf_spiel",
                            vec![
                                language::t("aafk_titel"),
                                language::t("aafk_hinweis"),
                                language::t("s_uc_titel"),
                                language::t("s_uc_eigene"),
                                language::t("s_uc_speichern"),
                                language::t("s_uc_neu"),
                                language::t("s_uc_ganz"),
                                language::t("s_uc_kanal"),
                            ],
                        ),
                        (
                            "hf_ordner",
                            vec![
                                language::t("s_uc_kanaele"),
                                format!("LIVE: {}", language::t("s_uc_ok")),
                                format!("PTU: {}", language::t("s_uc_ohne_sprache")),
                            ],
                        ),
                    ] {
                        page.go_to(key);
                        let (overflow, texts) = draw(&mut page, width, height);
                        let which = format!("{key} {} Größe {points} breit {wide} englisch {english}", scheme.id);
                        if overflow > 0.5 {
                            findings.push(format!("{which}: {overflow:.0} zu breit"));
                        }
                        for text in wanted {
                            // A button of the scheme KRT is in capitals.
                            let drawn = shows(&texts, &text) || shows(&texts, &text.to_uppercase());
                            assert!(drawn, "{which}: „{text}“ is not drawn: {texts:?}");
                        }
                    }
                }
            }
        }
    }
    theme::set_current("original");
    language::set_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(page_dir);
    assert!(findings.is_empty(), "{} findings: {:#?}", findings.len(), &findings[..findings.len().min(12)]);
}

#[test]
fn a_module_that_is_off_takes_its_pages_and_its_group_from_the_menu_and_a_jump_to_one_lands_on_modules() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("module-aus", "", &[]);
    let tall = |page: &mut SamplePage, key: &str| {
        page.go_to(key);
        draw(page, 1280.0, 2600.0).1
    };
    let (shops, trade) = (language::t("hf_laeden"), language::t("hf_gruppe_handel").to_uppercase());
    let texts = tall(&mut page, "hf_ordner");
    assert!(shows(&texts, &shops) && shows(&texts, &trade), "all is on at the start: {texts:?}");

    let paths = &page.store.as_ref().expect("real data").paths;
    assert!(versekit_core::modules::set_enabled(paths, "handel", false));
    page.reload_settings();
    let texts = tall(&mut page, "hf_ordner");
    assert!(!shows(&texts, &shops), "the pages of the module are gone from the menu: {texts:?}");
    assert!(!shows(&texts, &language::t("hf_verkauf")) && !shows(&texts, &language::t("hf_routen")), "{texts:?}");
    assert!(!shows(&texts, &trade), "and the group with them: {texts:?}");
    assert!(shows(&texts, &language::t("hf_hangar")), "the other modules stay: {texts:?}");
    assert!(shows(&texts, &language::t("hf_liste")), "and the core can not be switched off: {texts:?}");

    // A jump to a page of the module (from the blueprint list, a pinned tile): the page Modules says what is off.
    let texts = tall(&mut page, "hf_laeden");
    assert_eq!(page.page_key(), "hf_module", "{texts:?}");
    assert!(shows(&texts, &language::t("s_mo_lead")), "{texts:?}");
    let said = page.pin_notice.as_ref().map(|(_, text)| text.clone());
    let expected =
        language::tf("s_mo_aus_hinweis", &[("0", language::Arg::from(language::t("hf_gruppe_handel").as_str()))]);
    assert_eq!(said, Some(expected));

    // On again: everything is back, nothing was lost.
    let paths = &page.store.as_ref().expect("real data").paths;
    assert!(versekit_core::modules::set_enabled(paths, "handel", true));
    page.reload_settings();
    let texts = tall(&mut page, "hf_ordner");
    assert!(shows(&texts, &shops) && shows(&texts, &trade), "{texts:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// Coming back to the contracts after a new session was played: entering the page reads the logs again, the new
/// contract stands in the list.
#[test]
fn back_on_the_contracts_after_a_new_log_the_page_reads_it() {
    let _language = crate::language_lock();
    let dir = folder("auftraege-wiederkommen");
    theme::set_current("original");
    language::set_language(language::Choice::De);
    let mut page = SamplePage::with_settings(Paths::new(host_apart(&dir)));
    let backups = dir.join("LIVE").join("logbackups");
    std::fs::create_dir_all(&backups).unwrap();
    let log = |name: &str, id: &str, day: &str| {
        format!(
            "<2026-10-{day}T10:00:00.000Z> OnClientSpawned\n\
             <2026-10-{day}T10:00:05.000Z> <SHUDEvent_OnNotification> Added notification \"Auftrag angenommen: {name}: \" [1] MissionId: [{id}], ObjectiveId: []\n\
             <2026-10-{day}T10:00:09.000Z> <SHUDEvent_OnNotification> Added notification \"Auftrag abgeschlossen: {name}: \" [1] MissionId: [{id}], ObjectiveId: []\n"
        )
    };
    std::fs::write(backups.join("a.log"), log("Hunt", "m1", "01")).unwrap();
    let until = |page: &mut SamplePage, name: &str| {
        for _ in 0..200 {
            let texts = look(page, "hf_auftragslog");
            if shows(&texts, name) {
                return texts;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        look(page, "hf_auftragslog")
    };
    let texts = until(&mut page, "Hunt");
    assert!(shows(&texts, "Hunt") && !shows(&texts, "Rescue"), "{texts:?}");
    // Another session is played meanwhile, while another page is open.
    let _ = look(&mut page, "hf_liste");
    std::fs::write(backups.join("b.log"), log("Rescue", "m2", "02")).unwrap();
    let texts = until(&mut page, "Rescue");
    assert!(shows(&texts, "Rescue"), "the new contract after coming back: {texts:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_pinned_card_goes_with_its_module_and_comes_back_with_it() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("module-karte", "{\n  \"karten_angeheftet\": [\"laeden\", \"hangar\"]\n}\n", &[]);
    let cards = |page: &SamplePage| -> Vec<String> {
        page.tiles.iter().filter(|tile| tile.pinned).map(|tile| tile.key.clone()).collect()
    };
    let _ = look(&mut page, "");
    assert!(cards(&page).contains(&"hf_laeden".to_string()), "pinned at the start: {:?}", cards(&page));

    let paths = &page.store.as_ref().expect("real data").paths;
    assert!(versekit_core::modules::set_enabled(paths, "handel", false));
    page.reload_settings();
    let _ = look(&mut page, "");
    assert!(!cards(&page).contains(&"hf_laeden".to_string()), "gone with the module: {:?}", cards(&page));
    assert!(cards(&page).contains(&"hf_hangar".to_string()), "the card of another module stays");
    // The pin itself is kept: the file still names the page.
    let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
    assert!(file.contains("\"laeden\""), "{file}");

    let paths = &page.store.as_ref().expect("real data").paths;
    assert!(versekit_core::modules::set_enabled(paths, "handel", true));
    page.reload_settings();
    let _ = look(&mut page, "");
    assert!(cards(&page).contains(&"hf_laeden".to_string()), "back with the module: {:?}", cards(&page));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_credentials_of_a_given_data_folder_stay_inside_it() {
    let _language = crate::language_lock();
    // The usual places of the credentials lie elsewhere, in the folder of the real installation.
    let other = folder("geheim-fremd");
    let elsewhere = other.to_string_lossy().into_owned();
    let places = [
        ("HOME", elsewhere.as_str()),
        ("USERPROFILE", elsewhere.as_str()),
        ("LOCALAPPDATA", elsewhere.as_str()),
        ("XDG_CONFIG_HOME", elsewhere.as_str()),
    ];
    let (page, dir) = real_page("geheim", "", &places);
    let wire = page.connection.wire().expect("the connection of the program");
    // The folder the store of the connection works in, on every system (Windows seals the
    // credentials with its own key, but the sealed files sit in that folder too).
    let kept = versekit_core::basetool::secrets::folder(&wire.sync.paths().host);
    assert!(kept.starts_with(&*dir.to_string_lossy()), "{kept} lies outside {}", dir.display());
    // Where the credentials sit in a plain file, the page says where.
    assert_eq!(wire.file_store.as_deref(), (!cfg!(windows)).then_some(kept.as_str()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_sync_follows_the_data_folder_of_the_window() {
    let _language = crate::language_lock();
    let (mut page, first) = real_page("folgt", "", &[]);
    let second = folder("folgt-neu");
    page.use_paths(Paths::new(host_in(&second, &[])));
    let file = page.connection.wire().expect("wire").sync.paths().settings_file();
    assert!(file.starts_with(&*second.to_string_lossy()), "{file}");
    assert!(!file.starts_with(&*first.to_string_lossy()), "{file}");
    let _ = std::fs::remove_dir_all(first);
    let _ = std::fs::remove_dir_all(second);
}

#[test]
fn real_data_without_a_connection_say_so() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("ohne", "", &[]);
    // The connection could not be made.
    page.connection = crate::basetool_page::ConnectionState::new(None);
    page.org_stock = crate::org_stock_page::OrgStockState::new(None, None);
    let texts = look(&mut page, "k_basetool_verbindung");
    assert!(shows(&texts, &language::t("s_btv_ohne")), "the page says there is no connection");
    assert!(!shows(&texts, "Basetool nicht verbunden"), "no sample state");
    assert!(!shows(&texts, "PlayerOne"));
    assert!(page.page_hidden("hf_staffellager"), "no storage to show");

    // Counter-check: the sample page without a connection keeps its sample state.
    let mut sample = SamplePage { preview: true, ..SamplePage::new() };
    sample.connection = crate::basetool_page::ConnectionState::new(None);
    let texts = look(&mut sample, "k_basetool_verbindung");
    assert!(shows(&texts, "Basetool nicht verbunden") && !shows(&texts, &language::t("s_btv_ohne")));
    assert!(!sample.page_hidden("hf_staffellager"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_made_up_basetool_from_outside_stands_in_for_the_whole_connection() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("fremd", "", &[]);
    assert!(page.page_hidden("hf_staffellager"));
    page.use_storage_link(crate::org_stock_page::Link::demo());
    assert!(!page.page_hidden("hf_staffellager"), "the storage page is there");
    assert!(page.connection.wire().is_some_and(|wire| wire.demo.is_some()), "the connection page follows");
    let texts = look(&mut page, "k_basetool_verbindung");
    assert!(shows(&texts, &language::t("s_btv_beispiel")));
    // A new data folder does not take the connection from the program that gave it.
    let second = folder("fremd-neu");
    page.use_paths(Paths::new(host_in(&second, &[])));
    assert!(page.connection.wire().is_some_and(|wire| wire.demo.is_some()));
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_dir_all(second);
}

/// How many rows say that their function comes later.
fn not_yet_count(texts: &[(String, Pos2)]) -> usize {
    texts.iter().filter(|(text, _)| text == super::glance::NOT_YET).count()
}

#[test]
fn real_data_start_in_the_stored_language_and_drawing_leaves_it_alone() {
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let (mut page, dir) = real_page("sprache", "{\n  \"sprache\": \"en\"\n}\n", &[]);
    assert!(matches!(page.start_language(), language::Choice::En), "the stored English is the start language");
    // Drawing changes no language by itself: other windows and tests share it.
    let _ = look(&mut page, "hf_allgemein");
    assert_eq!(language::current(), language::Lang::De, "drawing switched the language");
    // Counter-check: the sample page starts German whatever is chosen.
    let mut sample = SamplePage { preview: true, ..SamplePage::new() };
    sample.general_english = true;
    assert!(matches!(sample.start_language(), language::Choice::De), "the sample page starts German");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn real_data_read_and_write_the_settings_of_allgemein_like_4_12() {
    let _language = crate::language_lock();
    let file = "{\n  \"update_pruefen\": false,\n  \"mangohud\": true,\n  \"signalton\": false,\n  \"pruefintervall_sekunden\": 7\n}\n";
    let (mut page, dir) = real_page("allgemein", file, &[]);
    assert!(page.mangohud && !page.sounds[0], "read with the keys of 4.12");
    assert_eq!(page.check_interval, 7, "the check interval of 4.12");

    // What cannot work yet says so; a switch that does nothing is not there.
    // Graphics card, second sound, volume and clock (the use count has no row any more; the start with the
    // system is a switch that writes its entry); on Linux MangoHud is a switch.
    // A window tall enough for the whole page: rows below the window are not drawn.
    page.go_to("hf_allgemein");
    let texts = draw(&mut page, 1280.0, 1600.0).1;
    assert_eq!(not_yet_count(&texts), 4, "{texts:?}");
    let reason = shows(&texts, "nur unter Linux");
    assert_eq!(reason, !cfg!(target_os = "linux"), "MangoHud: the reason where the system cannot");
    // Updates are always looked for quietly: no switch for it.
    assert!(!shows(&texts, "Nach Updates suchen"), "no update switch");
    // The check interval has its row with the stored number (at the end of the page: a tall window).
    let tall = draw(&mut page, 1280.0, 1600.0).1;
    assert!(shows(&tall, &language::t("s_takt")), "{tall:?}");
    assert!(shows(&tall, "7"), "{tall:?}");

    // A change is written under the key of 4.12, the rest of the file stays.
    page.mangohud = false;
    page.sounds[0] = true;
    page.check_interval = 12;
    let _ = look(&mut page, "hf_allgemein");
    let text = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
    for wanted in ["\"mangohud\": false", "\"signalton\": true", "\"pruefintervall_sekunden\": 12"] {
        assert!(text.contains(wanted), "{wanted} in {text}");
    }
    // The old update switch of 4.12 is left as it was, never written by v5.
    assert!(text.contains("\"update_pruefen\": false"), "{text}");

    // Counter-check: the sample page shows its switches and writes nothing.
    let mut sample = SamplePage { preview: true, ..SamplePage::new() };
    let texts = look(&mut sample, "hf_allgemein");
    assert_eq!(not_yet_count(&texts), 0, "the sample page keeps its switches");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_switch_of_the_start_with_the_system_is_the_entry_and_says_what_the_entry_says() {
    use versekit_core::autostart::{Memory, NAME};
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let (mut page, dir) = real_page("autostart", "{}\n", &[]);
    // In memory unless the program hands over the real one: nothing is written outside.
    assert!(!page.autostart_on);
    // The entry of 4.12 is there and the program says it is on, after it is handed over.
    let old = Memory::with(NAME, "\"C:\\Programs\\versekit.exe\"");
    page.use_autostart(Box::new(old), "\"C:\\Programs\\versekit.exe\"".to_string());
    assert!(page.autostart_on, "the switch shows the entry as it is");
    let off = page.change_autostart(false);
    assert!(!page.autostart_on, "off after switching it off");
    assert_eq!(off, language::tf("s_al_autostart", &[("0", language::Arg::from(language::t("e_aus").as_str()))]));
    let on = page.change_autostart(true);
    assert!(page.autostart_on);
    assert_eq!(on, language::tf("s_al_autostart", &[("0", language::Arg::from(language::t("e_an").as_str()))]));
    // A write that fails says so and the switch shows what the entry says, not what was wished for.
    page.use_autostart(Box::new(Memory::refusing()), String::new());
    let failed = page.change_autostart(true);
    assert_eq!(failed, language::t("ov_as_fehler"));
    assert!(!page.autostart_on, "still off");
    // The row is a switch on real data (no longer "not yet"), and where the system has none, the reason.
    let texts = look(&mut page, "hf_allgemein");
    assert!(!shows(&texts, &language::t("s_nicht_moegl")), "{texts:?}");
    page.use_autostart(Box::new(Memory::impossible()), String::new());
    let texts = look(&mut page, "hf_allgemein");
    assert!(shows(&texts, &language::t("s_nicht_moegl")), "{texts:?}");
    assert!(!page.use_copy_autostart(std::path::Path::new("/opt/v"), None), "the user's own data are no copy");
    let _ = std::fs::remove_dir_all(dir);
}

/// ⚠⚠ On a copy of the data the switch of the start with the system holds (it jumped back: the command was
/// empty), writes only into the copy, and its command starts the program on the copy.
#[test]
fn on_a_copy_the_switch_of_the_start_with_the_system_holds_and_its_entry_names_the_copy() {
    use versekit_core::autostart::{NAME, desktop_file_name};
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let dir = folder("autostart-kopie");
    let root = dir.to_string_lossy().into_owned();
    let paths = Paths::for_copy(host_in(&dir, &[]), &root).expect("an empty folder is a new copy");
    let mut page = SamplePage::with_settings(paths);
    assert!(page.use_copy_autostart(std::path::Path::new("/opt/V.AppImage"), None));
    let on = page.change_autostart(true);
    assert!(page.autostart_on, "the switch holds: {on}");
    assert_eq!(on, language::tf("s_al_autostart", &[("0", language::Arg::from(language::t("e_an").as_str()))]));
    let file = dir.join("AppData").join("autostart").join(desktop_file_name(NAME));
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let exec = text.lines().find_map(|l| l.strip_prefix("Exec=")).unwrap_or_default().replace("\\\\", "\\");
    assert!(exec.contains("--daten") && exec.contains(&root), "the entry starts the program on the copy: {exec}");
    assert!(!page.change_autostart(false).is_empty() && !page.autostart_on && !file.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn real_data_show_the_game_folder_that_is_there_and_say_when_it_is_not() {
    let _language = crate::language_lock();
    let dir = folder("spielordner");
    let live = dir.join("StarCitizen").join("LIVE");
    std::fs::create_dir_all(&live).unwrap();
    std::fs::write(live.join("Game.log"), "<2026-10-21T19:00:00.000Z> Log started\n").unwrap();
    std::fs::create_dir_all(dir.join("StarCitizen").join("PTU")).unwrap();
    let live_text = live.to_string_lossy().into_owned();
    let same = |a: &str, b: &str| a.replace('\\', "/") == b.replace('\\', "/");
    let file = format!("{{\n  \"spiel_ordner\": \"{}\"\n}}\n", live_text.replace('\\', "/"));
    let (mut page, home) = real_page("spiel", &file, &[]);
    let texts = look(&mut page, "hf_ordner");
    assert!(shows(&texts, &language::t("s_sc_da")), "found: it says so");
    assert!(texts.iter().any(|(text, _)| same(text, &live_text)), "the real path stands in the box");
    assert!(!shows(&texts, "Program Files"), "no made-up path");
    // Whether a channel is there stands on hover over its row now; the field of a channel found holds its folder.
    let ptu = dir.join("StarCitizen").join("PTU").to_string_lossy().into_owned();
    assert!(texts.iter().any(|(text, _)| same(text, &ptu)), "PTU found next to LIVE: {texts:?}");
    assert!(!shows(&texts, &language::t("s_pf_kanal_da")), "the state stands on hover, not as a line");
    let _ = std::fs::remove_dir_all(home);

    // What is not found depends on the computer: a folder without a Game.log, or nothing set, makes
    // the program look on the drives (Windows) or in the Wine prefixes of the home folder (the
    // throwaway folder here), so a game installed on this computer is found.
    let installed = game_on_this_computer();

    // An entered folder without a Game.log (the program reads nothing from it): not found, or the
    // installation of the computer where there is one.
    let empty = dir.join("leer");
    std::fs::create_dir_all(&empty).unwrap();
    let file = format!("{{\n  \"spiel_ordner\": \"{}\"\n}}\n", empty.to_string_lossy().replace('\\', "/"));
    let (mut page, home) = real_page("spiel-leer", &file, &[]);
    let texts = look(&mut page, "hf_ordner");
    assert_eq!(shows(&texts, &language::t("s_sc_da")), installed, "found only where the computer has a game");
    assert_eq!(shows(&texts, &language::t("s_sc_weg")), !installed, "{texts:?}");
    let _ = std::fs::remove_dir_all(home);

    if !installed {
        // Nothing set and nothing found: the honest "not found", no path of Windows.
        let (mut page, home) = real_page("spiel-weg", "", &[]);
        let texts = look(&mut page, "hf_ordner");
        assert!(shows(&texts, &language::t("s_sc_weg")) && !shows(&texts, &language::t("s_sc_da")));
        assert!(!shows(&texts, "Program Files") && !shows(&texts, "Spielordner gefunden"));
        let count = |key: &str| texts.iter().filter(|(text, _)| text == &language::t(key)).count();
        assert_eq!((count("s_pf_kanal_da"), count("s_pf_kanal_weg")), (0, 3));
        let _ = std::fs::remove_dir_all(home);

        // A path of Windows in the settings is not found on a system without it.
        let file = "{\n  \"spiel_ordner\": \"C:/Program Files/Roberts Space Industries/StarCitizen/LIVE\"\n}\n";
        let (mut page, home) = real_page("spiel-windows", file, &[]);
        let texts = look(&mut page, "hf_ordner");
        assert!(shows(&texts, &language::t("s_sc_weg")), "a path that is not there is not found");
        assert!(!shows(&texts, &language::t("s_sc_da")));
        let _ = std::fs::remove_dir_all(home);
    }

    // Counter-check: the sample page keeps the sample folder and its hints.
    let mut sample = SamplePage { preview: true, ..SamplePage::new() };
    let texts = look(&mut sample, "hf_ordner");
    assert!(shows(&texts, "Spielordner gefunden") && shows(&texts, "Program Files"));
    // Every channel has its row; that one is not installed stands on hover over it.
    assert!(shows(&texts, "HOTFIX") && shows(&texts, "EPTU") && !shows(&texts, "Nicht installiert"));
    let _ = std::fs::remove_dir_all(dir);
}

/// One frame of `page` in a window of `width`; every text with its rectangle and whether egui had to cut it.
fn rects(
    ctx: &egui::Context,
    page: &mut SamplePage,
    width: f32,
    height: f32,
) -> (f32, Vec<(String, egui::Rect, bool)>) {
    fn collect(shape: &egui::Shape, into: &mut Vec<(String, egui::Rect, bool)>) {
        match shape {
            egui::Shape::Text(text) => into.push((
                text.galley.text().to_string(),
                egui::Rect::from_min_size(text.pos, text.galley.size()),
                text.galley.elided,
            )),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, into)),
            _ => {}
        }
    }
    let (mut overflow, mut found) = (0.0, Vec::new());
    for _ in 0..3 {
        found.clear();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width, height))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| overflow = page.show(ui));
        output.textures_delta.clear();
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut found));
    }
    (overflow, found)
}

/// The sync button stands in the head of the three pages with an area, the line of its result under the head,
/// whole and wrapped, in both languages, at every size and in every scheme; the button keeps to the right
/// edge and clear of the heading.
#[test]
fn the_sync_button_and_its_line_fit_in_the_head_of_the_three_pages() {
    use std::sync::Arc;
    use versekit_core::basetool::exchange_double::ExchangeDouble;
    use versekit_core::basetool::scopes::{SETTING_BLUEPRINTS, SETTING_SHIPS, SETTING_STOCK};
    use versekit_core::jsonio::Value;

    let _language = crate::language_lock();
    let (mut page, dir) = real_page("kopf-sync", "", &[]);
    let double = Arc::new(ExchangeDouble::new());
    let wire =
        crate::basetool_page::Wire::over_double(&double, &dir.join("abgleich").to_string_lossy().replace('\\', "/"));
    for setting in [SETTING_BLUEPRINTS, SETTING_STOCK, SETTING_SHIPS] {
        assert!(wire.sync.set_setting(setting, Value::Bool(true)));
    }
    wire.sync.pass_now().expect("a pass");
    page.connection = crate::basetool_page::ConnectionState::new(Some(wire.clone()));
    let mut findings = Vec::new();
    // Two lines: the short one after a pass that went through, the long one of maintenance.
    for state in ["ok", "wartung"] {
        if state == "wartung" {
            double.fail("", 503, "SERVICE_UNAVAILABLE", Some(30), 1);
            let _ = wire.sync.pass_now();
        }
        for (key, area) in
            [("hf_liste", SETTING_BLUEPRINTS), ("hf_meinlager", SETTING_STOCK), ("hf_hangar", SETTING_SHIPS)]
        {
            page.go_to(key);
            let _ = area;
            for choice in [language::Choice::De, language::Choice::En] {
                language::set_language(choice);
                for scheme in theme::schemes().iter().step_by(3) {
                    for points in [font_size::POINTS_RANGE.0, font_size::POINTS_RANGE.1] {
                        let (min_width, height) = font_size::min_window(points);
                        for width in [min_width, 1800.0] {
                            theme::set_thread_current(scheme.id);
                            let ctx = egui::Context::default();
                            crate::install(&ctx);
                            page.size = points;
                            let (overflow, texts) = rects(&ctx, &mut page, width, height.max(600.0));
                            let at = format!("{state} {key} {choice:?} {} Größe {points} Breite {width}", scheme.id);
                            let word = language::t("s_bt_jetzt");
                            let Some((_, button, _)) = texts.iter().find(|(text, _, _)| *text == word) else {
                                findings.push(format!("{at}: kein Knopf"));
                                continue;
                            };
                            if overflow > 0.5 {
                                findings.push(format!("{at}: {overflow:.0} zu breit"));
                            }
                            if button.max.x > width {
                                findings.push(format!("{at}: der Knopf ragt über den rechten Rand"));
                            }
                            // The heading stands left of the button, in the same row.
                            let name = language::t(key).to_lowercase();
                            if let Some((_, heading, _)) = texts.iter().find(|(text, rect, _)| {
                                text.to_lowercase() == name && (rect.center().y - button.center().y).abs() < 8.0
                            }) && heading.max.x > button.min.x
                            {
                                findings.push(format!("{at}: der Knopf überdeckt die Überschrift"));
                            }
                            // The line stands whole under the head, wrapped, never cut.
                            let said =
                                if state == "ok" { language::t("s_btv_sync_ok") } else { language::t("s_btv_f_netz") };
                            let head_of_line = said.split('{').next().unwrap_or_default().to_string();
                            match texts.iter().find(|(text, _, _)| text.starts_with(&head_of_line)) {
                                None => findings.push(format!("{at}: keine Zeile")),
                                Some((_, line, elided)) => {
                                    if *elided {
                                        findings.push(format!("{at}: die Zeile ist abgeschnitten"));
                                    }
                                    if line.min.y < button.max.y || line.max.x > width {
                                        findings.push(format!(
                                            "{at}: die Zeile steht nicht unter dem Kopf oder ragt hinaus"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    language::set_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
    assert!(findings.is_empty(), "Kopf mit Abgleich:\n{}", findings.join("\n"));
}

/// The pages that show the placeholder of the sample page ("Hier steht die Seite.") instead of themselves.
fn placeholder_pages(page: &mut SamplePage) -> Vec<&'static str> {
    let placeholder = language::t("s_mu_seite");
    let mut found = Vec::new();
    for key in SamplePage::page_keys() {
        page.go_to(key);
        if draw(page, 1280.0, 820.0).1.iter().any(|(text, _)| *text == placeholder) {
            found.push(key);
        }
    }
    found
}

/// On real data every page draws itself: none shows the placeholder of the sample page, so a picture of a page
/// on real data is a picture of that page.
#[test]
fn on_real_data_no_page_shows_the_placeholder_of_the_sample() {
    let _language = crate::language_lock();
    language::set_language(language::Choice::De);
    let (mut page, dir) = real_page("kein-platzhalter", "", &[]);
    assert_eq!(placeholder_pages(&mut page), Vec::<&str>::new(), "pages on real data with the placeholder");
    let _ = std::fs::remove_dir_all(dir);
    // Counter-check: the sample page shows it for the pages only real data draw, Erkennung among them.
    let mut sample = SamplePage::preview();
    assert!(placeholder_pages(&mut sample).contains(&"hf_erkennung"), "the check sees the placeholder");
}
