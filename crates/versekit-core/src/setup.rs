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

//! What the setup assistant decides (`wizard.py` of 4.12): whether it has to run, which steps apply, and what
//! the game folder the player typed means. The window is in `versekit_ui`; this is the part that can be
//! checked without one.

use crate::jsonio::Value;
use crate::paths::Paths;

/// The proof that the program was set up here.
pub const DONE: &str = "einrichtung_fertig";
/// The player went on without a game folder on purpose, and does not want the assistant at every start.
pub const WITHOUT_GAME: &str = "einrichtung_ohne_spiel";
/// The game folder as the player confirmed it.
pub const GAME_FOLDER: &str = "spiel_ordner";
/// Which version of the setup the player has seen: the short one for those who set the program up before this
/// version (`Willkommen in 5.0`), the full one writes it too, so that nobody gets both.
pub const VERSION_KEY: &str = "einrichtung_version";
/// The version of the setup now. A player whose [`VERSION_KEY`] is lower (or missing) and who set the program up
/// before is shown the short version once.
pub const VERSION: i64 = 5;

/// The steps of the assistant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The first page of the short setup: what 5.0 is, and that the data and settings were taken over.
    Welcome,
    /// The language of the window.
    Language,
    /// Where the data lies.
    Storage,
    /// Where Star Citizen is, which channels are installed, and (read at once) the blueprints of the sessions kept
    /// so far.
    Game,
    /// The own account: the one found in the logs to confirm, or the choice when there are several.
    Account,
    /// The overlay in the game: when it shows and how opaque it is; with a picture.
    Overlay,
    /// Text size, play time, colour scheme, blueprint range.
    Display,
    /// The texts in the game: the notes and the translation that carries them; with a before and after.
    Texts,
    /// What the program can be connected to: the Hangar-Exporter in the browser, the Basetool (optional).
    Connections,
    /// Start with the system and the icon next to the clock.
    Start,
    /// The end.
    Done,
}

/// The steps that apply now, in this order. ⚠ A list, not fixed numbers: without a game folder the texts of the
/// game are gone, and where there is neither a start with the system nor an icon the card for it is gone. With
/// fixed numbers a page would stand there empty and the counter would lie. (4.12 had nine steps: the reading of
/// the logs is part of the game step now, and the notes and the translation are one step.)
pub fn steps(without_game: bool, start_possible: bool) -> Vec<Step> {
    let mut steps = vec![Step::Language, Step::Storage, Step::Game];
    // Without a game there are no logs and no account in them.
    if !without_game {
        steps.push(Step::Account);
    }
    steps.extend([Step::Overlay, Step::Display]);
    if !without_game {
        steps.push(Step::Texts);
    }
    steps.push(Step::Connections);
    if start_possible {
        steps.push(Step::Start);
    }
    steps.push(Step::Done);
    steps
}

/// Has the program been set up here before? ⚠ NOT told by the read position of the game log: there is a
/// button that deletes it on purpose, and the complete assistant would come at the next start though nothing
/// is missing. The proof is the setting `einrichtung_fertig`. Older installations do not have it, so a game
/// folder that is ENTERED counts too: it stands in the settings only when somebody confirmed it (the assistant
/// or the page "Erkennung"); one that was only found does not count, or a new player with the game installed
/// would never see the assistant.
pub fn is_configured(paths: &Paths) -> bool {
    paths.setting_bool(DONE, false) || paths.setting(GAME_FOLDER).is_some_and(|folder| !folder.is_empty())
}

/// Can the program write into its data folder? Otherwise it remembers nothing, not even that it is set up
/// (a data folder in OneDrive that Windows locks).
pub fn storage_writable(paths: &Paths) -> bool {
    paths.storage_status(&paths.app_folder()).writable
}

/// Does the assistant have to run? At the first start, or when the game is missing. ⚠ Whoever went on without
/// a game folder on purpose does not get it at every start: before, the question hung on the game found, and
/// on a computer without Star Citizen that meant starting over every time. And FIRST: can anything be saved at
/// all? If not, the assistant leads to the step "Datenordner".
pub fn needed(paths: &Paths) -> bool {
    if !storage_writable(paths) {
        return true;
    }
    if paths.setting_bool(WITHOUT_GAME, false) {
        return false;
    }
    !is_configured(paths) || paths.game_folder().is_none()
}

/// What the text typed for the game folder means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameFolder {
    /// Nothing typed.
    Empty,
    /// A folder with a `Game.log`; `deduced` when it is not the folder typed (a file, a channel folder, a
    /// folder above).
    Found { folder: String, deduced: bool },
    /// Nothing usable below what was typed.
    NoLog,
}

/// Reads what was typed (`wizard._check_path`): the folder that holds the `Game.log`, if there is one.
pub fn check_game_folder(paths: &Paths, typed: &str) -> GameFolder {
    let typed = typed.trim();
    if typed.is_empty() {
        return GameFolder::Empty;
    }
    match paths.resolve_game_folder(typed) {
        Some(folder) => {
            let trim = |s: &str| s.trim_end_matches(['/', '\\']).to_string();
            let deduced = trim(&folder) != trim(typed);
            GameFolder::Found { folder, deduced }
        }
        None => GameFolder::NoLog,
    }
}

/// Remembers the game folder the player confirmed. A folder that was typed again after "without game"
/// switches that off: the player wants the game now.
pub fn confirm_game_folder(paths: &Paths, folder: &str) -> bool {
    let wrote = paths.set_setting(GAME_FOLDER, Value::str(folder));
    paths.set_setting(WITHOUT_GAME, Value::Bool(false)) && wrote
}

/// Goes on without a game folder, once and on purpose (`wizard._without_game`).
pub fn go_on_without_game(paths: &Paths) -> bool {
    paths.set_setting(WITHOUT_GAME, Value::Bool(true))
}

/// The assistant ran to its end: it does not come again by itself, and the short version is not needed.
pub fn finish(paths: &Paths) -> bool {
    let done = paths.set_setting(DONE, Value::Bool(true));
    mark_version(paths) && done
}

/// Notes that the player has seen the setup of this version (the full one, or the short one).
pub fn mark_version(paths: &Paths) -> bool {
    paths.set_setting(VERSION_KEY, Value::int(VERSION))
}

/// Does the short setup `Willkommen in 5.0` have to come? For a player who set the program up before this
/// version and has not seen it yet. Not for a program that needs the full assistant (not set up, nothing can be
/// saved, the game folder gone): that one covers it.
pub fn short_needed(paths: &Paths) -> bool {
    !needed(paths) && is_configured(paths) && paths.setting_int(VERSION_KEY, 0, None, None) < VERSION
}

/// The steps of the short setup: what is new to look at, filled in with what the player has. Without a game
/// folder the texts of the game are left out, as in the full one.
pub fn short_steps(without_game: bool) -> Vec<Step> {
    let mut steps = vec![Step::Welcome];
    // The own account is told by its number now: a player of 4.12 confirms it once.
    if !without_game {
        steps.push(Step::Account);
    }
    steps.extend([Step::Overlay, Step::Display]);
    if !without_game {
        steps.push(Step::Texts);
    }
    steps.extend([Step::Connections, Step::Done]);
    steps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    /// A throwaway data folder (`SC_BP_HOME`) and a throwaway home, and a game folder given from outside that
    /// holds no log: whatever Star Citizen this computer has, it is not found. `with_game` puts a log in.
    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-einrichtung-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let host = Host::new(
            Flavor::native(),
            &[(crate::legacy::ENV_HOME, &text), ("HOME", &text), ("USERPROFILE", &text)],
            &text,
        );
        let paths = Paths::new(host);
        let nowhere = dir.join("kein-spiel");
        std::fs::create_dir_all(&nowhere).unwrap();
        paths.set_given_game(&nowhere.to_string_lossy(), crate::given::GivenBy::Named("Pruefung".into()));
        (paths, dir)
    }

    /// Gives the paths a game folder with a log; returns the folder.
    fn with_game(paths: &Paths, dir: &std::path::Path) -> std::path::PathBuf {
        let game = dir.join("StarCitizen").join("LIVE");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Game.log"), "").unwrap();
        paths.set_given_game(&game.to_string_lossy(), crate::given::GivenBy::Named("Pruefung".into()));
        game
    }

    #[test]
    fn the_steps_that_apply_follow_the_game_and_the_start() {
        use Step::*;
        assert_eq!(
            steps(false, true),
            [Language, Storage, Game, Account, Overlay, Display, Texts, Connections, Start, Done],
            "with a game folder: all ten"
        );
        assert_eq!(
            steps(true, true),
            [Language, Storage, Game, Overlay, Display, Connections, Start, Done],
            "without: the account and the texts of the game are gone"
        );
        assert_eq!(steps(false, false), [Language, Storage, Game, Account, Overlay, Display, Texts, Connections, Done]);
        assert_eq!(steps(true, false), [Language, Storage, Game, Overlay, Display, Connections, Done]);
        assert!(steps(false, true).last() == Some(&Done) && steps(true, false).first() == Some(&Language));
    }

    #[test]
    fn the_short_setup_has_its_own_steps_and_leaves_the_texts_of_the_game_out_without_a_game() {
        use Step::*;
        assert_eq!(short_steps(false), [Welcome, Account, Overlay, Display, Texts, Connections, Done]);
        assert_eq!(short_steps(true), [Welcome, Overlay, Display, Connections, Done]);
        // The full setup never starts with the welcome page.
        for without_game in [false, true] {
            for start in [false, true] {
                assert!(!steps(without_game, start).contains(&Welcome));
            }
        }
    }

    #[test]
    fn the_short_setup_comes_once_for_a_program_set_up_before_and_never_for_a_new_one() {
        let (paths, dir) = paths_in("kurz");
        // New: the full assistant, not the short one.
        assert!(needed(&paths) && !short_needed(&paths), "a new program has the full assistant");
        // Set up with 4.12 (the proof, a game folder entered, no version): the short one.
        let game = with_game(&paths, &dir);
        assert!(paths.set_setting(GAME_FOLDER, Value::str(game.to_string_lossy().into_owned())));
        assert!(paths.set_setting(DONE, Value::Bool(true)));
        assert!(!needed(&paths) && short_needed(&paths), "a program set up before is shown the short one");
        // Seen: not again.
        assert!(mark_version(&paths));
        assert!(!short_needed(&paths), "once");
        // An older version number comes again; the number of this version does not.
        assert!(paths.set_setting(VERSION_KEY, Value::int(VERSION - 1)));
        assert!(short_needed(&paths));
        assert!(paths.set_setting(VERSION_KEY, Value::int(VERSION)));
        assert!(!short_needed(&paths));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_full_assistant_writes_the_version_too_so_that_nobody_gets_both() {
        let (paths, dir) = paths_in("kurz-voll");
        assert!(finish(&paths));
        assert_eq!(paths.setting_int(VERSION_KEY, 0, None, None), VERSION);
        assert!(paths.setting_bool(DONE, false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_program_that_cannot_save_or_has_lost_its_game_folder_has_the_full_assistant_not_the_short_one() {
        let (paths, dir) = paths_in("kurz-voll-noetig");
        // Set up before, but the game folder is gone and no "without game" was chosen: the full one covers it.
        assert!(paths.set_setting(DONE, Value::Bool(true)));
        assert!(paths.set_setting(GAME_FOLDER, Value::str("/gibt/es/nicht")));
        assert!(needed(&paths), "the game is missing");
        assert!(!short_needed(&paths), "so the short setup does not come");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_program_is_set_up_by_the_proof_or_by_an_entered_game_folder_not_by_a_found_one() {
        let (paths, dir) = paths_in("eingerichtet");
        assert!(!is_configured(&paths), "new");
        // A game that is found but was never confirmed does not count, or a new player would never see it.
        with_game(&paths, &dir);
        assert!(paths.game_folder().is_some(), "the game is found");
        assert!(!is_configured(&paths), "found is not entered");
        assert!(paths.set_setting(DONE, Value::Bool(true)));
        assert!(is_configured(&paths), "the proof");
        let (older, dir2) = paths_in("aeltere");
        assert!(older.set_setting(GAME_FOLDER, Value::str("D:/Spiele/LIVE")));
        assert!(is_configured(&older), "an older installation: an entered game folder");
        let (empty, dir3) = paths_in("leer");
        assert!(empty.set_setting(GAME_FOLDER, Value::str("")));
        assert!(!is_configured(&empty), "an empty entry is none");
        for d in [dir, dir2, dir3] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn what_the_assistant_writes_is_written() {
        let (paths, dir) = paths_in("schreiben");
        let game = with_game(&paths, &dir);
        assert!(confirm_game_folder(&paths, &game.to_string_lossy()));
        assert_eq!(paths.setting(GAME_FOLDER).as_deref(), Some(game.to_string_lossy().as_ref()), "the folder is kept");
        assert!(!paths.setting_bool(DONE, false));
        assert!(finish(&paths));
        assert!(paths.setting_bool(DONE, false), "the proof is written");
        assert!(go_on_without_game(&paths));
        assert!(paths.setting_bool(WITHOUT_GAME, false), "the way out is written");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_assistant_is_needed_at_the_first_start_or_without_the_game_but_not_after_going_on_without() {
        let (paths, dir) = paths_in("noetig");
        assert!(needed(&paths), "first start");
        // Set up, with a game folder that is there.
        let game = with_game(&paths, &dir);
        assert!(confirm_game_folder(&paths, &game.to_string_lossy()));
        assert!(finish(&paths));
        assert!(!needed(&paths), "set up and the game is there");
        // The game folder is gone: the assistant asks again ...
        std::fs::remove_file(game.join("Game.log")).unwrap();
        assert!(paths.game_folder().is_none(), "no game is found");
        assert!(needed(&paths), "the game is missing");
        // ... unless the player went on without on purpose.
        assert!(go_on_without_game(&paths));
        assert!(!needed(&paths), "on purpose and once, whether or not a game is found");
        // A game folder entered afterwards switches that off again.
        std::fs::write(game.join("Game.log"), "").unwrap();
        assert!(confirm_game_folder(&paths, &game.to_string_lossy()));
        assert!(!paths.setting_bool(WITHOUT_GAME, true));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn where_nothing_can_be_saved_the_assistant_is_needed_even_after_going_on_without() {
        let dir = std::env::temp_dir().join(format!("versekit-einrichtung-{}-gesperrt", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // A file where the data folder should be: nothing can be written.
        let blocked = dir.join("kein-ordner");
        std::fs::write(&blocked, "Datei").unwrap();
        let text = blocked.to_string_lossy().into_owned();
        let host = Host::new(
            Flavor::native(),
            &[(crate::legacy::ENV_HOME, &text), ("HOME", &text), ("USERPROFILE", &text)],
            &text,
        );
        let paths = Paths::new(host);
        assert!(!storage_writable(&paths));
        assert!(needed(&paths), "nothing can be saved: first of all the assistant");
        // Where the data folder is fine, the same state would not need it.
        let (writable, dir2) = paths_in("beschreibbar");
        assert!(storage_writable(&writable));
        assert!(writable.set_setting(WITHOUT_GAME, Value::Bool(true)));
        assert!(!needed(&writable));
        for d in [dir, dir2] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn what_is_typed_for_the_game_folder_is_read_as_the_folder_with_the_log() {
        let (paths, dir) = paths_in("spielordner");
        let live = dir.join("StarCitizen").join("LIVE");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::write(live.join("Game.log"), "").unwrap();
        let live_text = live.to_string_lossy().into_owned();
        assert_eq!(check_game_folder(&paths, "  "), GameFolder::Empty);
        assert_eq!(
            check_game_folder(&paths, &live_text),
            GameFolder::Found { folder: live_text.clone(), deduced: false }
        );
        // The folder above the channels: the channel below it is taken, and it is said so.
        let above = dir.join("StarCitizen").to_string_lossy().into_owned();
        match check_game_folder(&paths, &above) {
            GameFolder::Found { folder, deduced } => {
                assert!(deduced && folder.ends_with("LIVE"), "{folder}");
            }
            other => panic!("{other:?}"),
        }
        // The file itself: its folder.
        let file = live.join("Game.log").to_string_lossy().into_owned();
        assert!(matches!(check_game_folder(&paths, &file), GameFolder::Found { deduced: true, .. }));
        // A trailing separator is no deduction.
        let slashed = format!("{live_text}{}", std::path::MAIN_SEPARATOR);
        assert!(matches!(check_game_folder(&paths, &slashed), GameFolder::Found { deduced: false, .. }));
        // Nothing with a log below.
        let nowhere = dir.join("leer");
        std::fs::create_dir_all(&nowhere).unwrap();
        assert_eq!(check_game_folder(&paths, &nowhere.to_string_lossy()), GameFolder::NoLog);
        let _ = std::fs::remove_dir_all(dir);
    }
}
