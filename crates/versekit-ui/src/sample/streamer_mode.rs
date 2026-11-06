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

//! Streamer mode on every page: with made-up private values in the files of a throwaway folder and
//! in the made-up Basetool, every page of the window is drawn in both languages, and none of the
//! values may stand in a text it draws (or in a name a screen reader hears). The same pages with
//! the mode off must show them, or the check would be green for nothing.

use std::collections::BTreeSet;

use versekit_core::language::{self, Choice};
use versekit_core::paths::Paths;
use versekit_core::streamer;

use super::SamplePage;
use super::english::{drawn_texts, pages};
use super::real_data::{folder, host_in};

/// The handle of the player, in the log of the game and as the own account.
const HANDLE: &str = "Quarkenpilot77";
/// The name the player gives as reporter.
const REPORTER: &str = "Meldefritz Zornig";
/// The name of this installation in the Basetool.
const CONNECTION: &str = "Rechenknecht Delta";
/// The start command of the game: a path to a program.
const STARTER: &str = r"D:\Spiele\Geheim\Starter.exe";
/// The front part of the id of a game controller.
const DEVICE: &str = "7A3C91E0";
/// The pairing code of the hangar delivery.
const TOKEN: &str = "c0ffee0123456789abcdef0123456789c0ffee0123456789abcdef0123456789";
/// Units and members of the made-up Basetool (`basetool/demo.rs`), which the page reads and must hide.
const DEMO_PRIVATE: [&str; 9] =
    ["Staffel Alpha", "Frachtkommando", "Bereich Industrie", "Kestrel", "Orbit", "PlayerOne", "PlayerTwo", "Vega", "Nova"];

/// Pages the made-up Basetool does not fill with a person.
struct World {
    page: SamplePage,
    dir: std::path::PathBuf,
    /// Everything that must not be seen while the mode is on.
    private: Vec<String>,
}

fn world(name: &str) -> World {
    let dir = folder(name);
    let game = dir.join("LIVE");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(
        game.join("Game.log"),
        format!(
            "<Legacy login response> [CIG-net] User Login Success - Handle[{HANDLE}] - Time\n\
             Connected joystick1: VKB Gladiator {{{DEVICE}-0000-0000-0000-504944564944}}\n"
        ),
    )
    .unwrap();
    let escaped = STARTER.replace('\\', "\\\\");
    std::fs::write(
        dir.join("einstellungen.json"),
        format!(
            "{{\n  \"eigener_account\": \"{HANDLE}\",\n  \"melder_name\": \"{REPORTER}\",\n  \"basetool_name\": \"{CONNECTION}\",\n  \"spielstarter\": \"{escaped}\",\n  \"hangar_einlieferung_token\": \"{TOKEN}\"\n}}\n"
        ),
    )
    .unwrap();
    let game_text = game.to_string_lossy().into_owned();
    let vars = [(versekit_core::paths::ENV_INSTALL_DIR, game_text.as_str())];
    // What the window does with the setting, done by hand: the mode itself is set per thread, so that
    // the tests which run side by side do not see each other's.
    streamer::learn_paths(&Paths::new(host_in(&dir, &vars)));
    let mut page = SamplePage::with_settings(Paths::new(host_in(&dir, &vars)));
    page.use_storage_link(crate::org_stock_page::Link::demo());
    let mut private: Vec<String> = [HANDLE, REPORTER, CONNECTION, STARTER, DEVICE, TOKEN]
        .into_iter()
        .map(str::to_string)
        .collect();
    private.push(dir.to_string_lossy().into_owned());
    private.push(dir.file_name().unwrap().to_string_lossy().into_owned());
    private.push(game_text);
    private.extend(DEMO_PRIVATE.iter().map(|s| s.to_string()));
    World { page, dir, private }
}

/// Every text the pages draw (and every name a screen reader hears), with the page that drew it.
/// The reads of the pages run in threads of their own: each page is drawn a few times.
fn drawn(page: &mut SamplePage) -> Vec<(&'static str, String)> {
    let mut all = Vec::new();
    for key in pages() {
        page.go_to(key);
        for (width, height) in [(1280.0, 900.0), (700.0, 700.0)] {
            for _ in 0..4 {
                for text in drawn_texts(page, width, height) {
                    all.push((key, text));
                }
                std::thread::sleep(std::time::Duration::from_millis(12));
            }
        }
    }
    all
}

/// Does `value` stand in `text` as a word of its own (a name inside a longer word, `Orbit` in
/// `Orbituary`, is not the name)? A value with a separator in it (a path, a code) counts anywhere.
fn stands_alone(text: &str, value: &str) -> bool {
    let anywhere = value.contains(['\\', '/', ':', '.', '@', '-']);
    text.match_indices(value).any(|(start, found)| {
        anywhere
            || (!text[..start].chars().next_back().is_some_and(char::is_alphanumeric)
                && !text[start + found.len()..].chars().next().is_some_and(char::is_alphanumeric))
    })
}

fn show_of(texts: &[(&'static str, String)], private: &[String]) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (page, text) in texts {
        let lower = text.to_lowercase();
        for value in private {
            if stands_alone(&lower, &value.to_lowercase()) {
                found.insert(format!("{value}  <-  {page}: {text}"));
            }
        }
    }
    found
}

/// Streamer mode off: the made-up values really are on the pages (so the check below can fail).
/// On: not one of them is, in either language, and the placeholder stands in their place.
#[test]
fn no_page_shows_a_private_value_while_the_streamer_mode_is_on() {
    let _language = crate::language_lock();
    let mut world = world("streamer");
    for (choice, placeholder) in [(Choice::De, "[Geschwärzt]"), (Choice::En, "[Redacted]")] {
        language::set_thread_language(choice);

        streamer::set_thread_active(Some(false));
        // The reads of the pages run in threads of their own: drawn again until what they read is on the pages
        // (on a busy machine that takes long), at most a minute.
        let folder = world.dir.file_name().unwrap().to_string_lossy().into_owned();
        let musts = [HANDLE, DEMO_PRIVATE[0], folder.as_str()];
        let end = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let shown: Vec<String> = loop {
            let open = drawn(&mut world.page);
            let shown: Vec<String> = show_of(&open, &world.private)
                .into_iter()
                .map(|line| line.split("  <-  ").next().unwrap().to_string())
                .collect();
            if musts.iter().all(|must| shown.iter().any(|value| value == must)) || std::time::Instant::now() > end {
                break shown;
            }
        };
        for must in musts {
            assert!(shown.iter().any(|value| value == must), "{choice:?}: without the mode {must} is not on a page: the check proves nothing\n{shown:?}");
        }

        streamer::set_thread_active(Some(true));
        let covered = drawn(&mut world.page);
        let leaks = show_of(&covered, &world.private);
        streamer::set_thread_active(None);
        language::set_thread_language(Choice::Auto);
        assert!(leaks.is_empty(), "{choice:?}: private values on the pages:\n{}", leaks.into_iter().collect::<Vec<_>>().join("\n"));
        let marks = covered.iter().filter(|(_, text)| text.contains(placeholder)).count();
        assert!(marks > 5, "{choice:?}: the placeholder {placeholder} stands only {marks} times");
    }
    let _ = std::fs::remove_dir_all(&world.dir);
}

/// The switch is off from the factory and is kept in the settings file.
#[test]
fn the_switch_is_off_from_the_factory_and_is_stored() {
    let _language = crate::language_lock();
    let dir = folder("streamer-schalter");
    let mut page = SamplePage::with_settings(Paths::new(host_in(&dir, &[])));
    assert!(!page.streamer, "off from the factory");
    assert!(!streamer::load(&Paths::new(host_in(&dir, &[]))));
    let values = page.setting_values();
    assert!(
        values.iter().any(|(key, value)| *key == streamer::SETTING && *value == versekit_core::jsonio::Value::Bool(false)),
        "the setting is written with the others"
    );
    page.streamer = true;
    assert!(
        page.setting_values()
            .iter()
            .any(|(key, value)| *key == streamer::SETTING && *value == versekit_core::jsonio::Value::Bool(true))
    );
    let _ = std::fs::remove_dir_all(dir);
}
