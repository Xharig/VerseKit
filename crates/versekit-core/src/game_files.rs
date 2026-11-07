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

//! The one way every page that changes a file of the game goes (`attributes.xml`, `actionmaps.xml`,
//! `GraphicsSettings.json`, `user.cfg`; the large `global.ini` has its own number of copies in
//! `translation`): a copy beside the file first, then the writing, then the old copies cut.
//!
//! The rules (decided in the night of 2026-10-22, the player may overturn them):
//! * **One name for all new copies:** `<file>.versekit-<YYYYMMDD-HHMMSS>`, two within the same second
//!   `-02`, `-03` … (nothing is overwritten). Copies of 4.12 (`.scbpw-…`) are found and counted, can be put
//!   back, but are never made again.
//! * **Ten copies** of a small file stay (the large `global.ini` keeps three).
//! * **Nothing is written while the game runs, or when nobody can tell** (`game_start::run_state`): a game
//!   that has the file open reads it again, or writes its own over it.
//! * A writing that fails takes its copy back, so that failed tries do not push the real ones out.
//!
//! ⚠ Writing in a file of the real installation is for the player's click only. A test writes into a
//! throwaway folder, never into the installation, the test copy or the rebuilt game data.

use std::io;

use crate::game_start::{self, RunState};
use crate::language::t;
use crate::paths::Paths;
use crate::translation;

/// So many copies of a small file stay.
pub const KEPT: usize = 10;

/// Why a file was not written.
#[derive(Debug)]
pub enum WriteError {
    /// The game runs.
    GameRuns,
    /// Nobody can tell whether the game runs: counts as running.
    GameUnknown,
    /// The copy or the writing failed.
    Failed(io::Error),
}

impl WriteError {
    /// The reason in the language of the program, with what to do about it.
    pub fn reason(&self) -> String {
        match self {
            WriteError::GameRuns => t("s_gf_spiel"),
            WriteError::GameUnknown => t("s_gf_unklar"),
            WriteError::Failed(error) => {
                crate::language::tf("s_gf_fehler", &[("0", crate::language::Arg::from(error.to_string().as_str()))])
            }
        }
    }
}

/// Is the game quiet? `Ok` when the files of the game may be written now.
pub fn check_quiet(paths: &Paths) -> Result<(), WriteError> {
    match game_start::run_state(paths) {
        RunState::Idle => Ok(()),
        RunState::Running => Err(WriteError::GameRuns),
        RunState::Unknown => Err(WriteError::GameUnknown),
    }
}

/// Replaces the file of the game with `bytes`: a copy first, the writing, then the old copies cut to
/// [`KEPT`]. Gives the copy (`None` where the file was not there yet). The game must be quiet.
///
/// # Errors
///
/// [`WriteError`]: the game runs or cannot be told, or the copy or the writing failed (the file is as it was).
pub fn write(paths: &Paths, target: &str, bytes: &[u8]) -> Result<Option<String>, WriteError> {
    // One writer at a time: two copies made at once would take the same name.
    let _one_writer = crate::injection::game_file_lock();
    write_locked(paths, target, bytes)
}

/// [`write`] for a caller that holds `injection::game_file_lock()` already: it read the file, worked out the new
/// text and writes it all under one lock, so no other writer slips in between.
///
/// # Errors
///
/// [`WriteError`]: the game runs or cannot be told, or the copy or the writing failed (the file is as it was).
pub fn write_locked(paths: &Paths, target: &str, bytes: &[u8]) -> Result<Option<String>, WriteError> {
    check_quiet(paths)?;
    translation::replace_game_file(paths, target, bytes, KEPT).map_err(WriteError::Failed)
}

/// The copies of the file, oldest first (the ones of 4.12 included).
pub fn backups(target: &str) -> Vec<String> {
    translation::backups_of(target)
}

/// Puts the newest copy back. Gives the copy used. The game must be quiet.
///
/// # Errors
///
/// [`WriteError`]: the game runs or cannot be told, or there is no copy or it cannot be written back.
pub fn restore_newest(paths: &Paths, target: &str) -> Result<String, WriteError> {
    check_quiet(paths)?;
    translation::restore_backup(paths, target, false).map_err(WriteError::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{ENV_INSTALL_DIR, Host};
    use crate::pypath::Flavor;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-gamefiles-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    /// A user on Linux whose game is quiet (a log written `age` seconds ago; `None`: no game folder known).
    fn world(name: &str, age: Option<u64>) -> (String, Paths, String) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        std::fs::create_dir_all(&game).expect("folder");
        let mut vars = vec![("SC_BP_HOME", home.as_str()), ("HOME", home.as_str()), (game_start::ENV_NO_PROCESS, "1")];
        if let Some(age) = age {
            std::fs::write(format!("{game}/Game.log"), "x").expect("log");
            let at = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
            std::fs::File::options()
                .write(true)
                .open(format!("{game}/Game.log"))
                .and_then(|f| f.set_modified(at))
                .expect("age");
            vars.push((ENV_INSTALL_DIR, game.as_str()));
        }
        let paths = Paths::new(Host::new(Flavor::Posix, &vars, &home));
        let target = format!("{game}/user/client/0/Profiles/default/attributes.xml");
        std::fs::create_dir_all(std::path::Path::new(&target).parent().expect("parent")).expect("folder");
        (home, paths, target)
    }

    #[test]
    fn a_write_keeps_a_copy_and_ten_stay_and_nothing_is_written_over() {
        let (home, paths, target) = world("zehn", Some(7200));
        std::fs::write(&target, "version 0").expect("file");
        for n in 1..=13 {
            let backup = write(&paths, &target, format!("version {n}").as_bytes()).expect("written").expect("a copy");
            assert_eq!(
                std::fs::read_to_string(&backup).unwrap(),
                format!("version {}", n - 1),
                "the copy holds what was there"
            );
            assert!(backup.contains(".versekit-"), "{backup}");
        }
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "version 13");
        let kept = backups(&target);
        assert_eq!(kept.len(), KEPT, "{kept:?}");
        // Within the same second the names still differ (13 writes took far less than 13 seconds).
        let mut unique = kept.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), kept.len());
        // The newest copy is the one before the last writing; the oldest still there is three writings back.
        assert_eq!(std::fs::read_to_string(kept.last().unwrap()).unwrap(), "version 12");
        assert_eq!(std::fs::read_to_string(&kept[0]).unwrap(), "version 3");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn copies_of_4_12_count_and_go_first_but_are_never_made_again() {
        let (home, paths, target) = world("alt", Some(7200));
        std::fs::write(&target, "now").expect("file");
        for n in 0..10 {
            std::fs::write(format!("{target}.scbpw-20260101-0000{n:02}"), format!("old {n}")).expect("old copy");
        }
        let backup = write(&paths, &target, b"next").expect("written").expect("a copy");
        assert!(backup.contains(".versekit-") && !backup.contains(".scbpw-"), "{backup}");
        let kept = backups(&target);
        assert_eq!(kept.len(), KEPT, "{kept:?}");
        assert!(!kept.iter().any(|b| b.ends_with("20260101-000000")), "the oldest old copy was cut: {kept:?}");
        assert_eq!(kept.iter().filter(|b| b.contains(".scbpw-")).count(), 9, "{kept:?}");
        let name = |path: &str| path.rsplit(['/', '\\']).next().unwrap_or_default().to_string();
        assert_eq!(kept.last().map(|b| name(b)), Some(name(&backup)), "the new one is the newest");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn nothing_is_written_while_the_game_runs_or_when_nobody_can_tell() {
        for (name, age, expect_runs) in [("laeuft", Some(5), true), ("unklar", None, false)] {
            let (home, paths, target) = world(name, age);
            std::fs::write(&target, "as it was").expect("file");
            let error = write(&paths, &target, b"changed").expect_err("refused");
            assert!(matches!(error, WriteError::GameRuns) == expect_runs, "{name}: {error:?}");
            assert!(matches!(error, WriteError::GameUnknown) != expect_runs, "{name}: {error:?}");
            assert_eq!(std::fs::read_to_string(&target).unwrap(), "as it was", "{name}: the file is untouched");
            assert!(backups(&target).is_empty(), "{name}: not even a copy");
            assert!(matches!(restore_newest(&paths, &target), Err(WriteError::GameRuns | WriteError::GameUnknown)));
            let said = error.reason();
            assert!(!said.is_empty() && !said.contains('—'), "{said}");
            let _ = std::fs::remove_dir_all(home);
        }
    }

    #[test]
    fn a_file_that_is_not_there_yet_is_written_without_a_copy_and_the_newest_copy_comes_back() {
        let (home, paths, target) = world("neu", Some(7200));
        assert_eq!(write(&paths, &target, b"first").expect("written"), None, "nothing to keep");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "first");
        write(&paths, &target, b"second").expect("written");
        write(&paths, &target, b"third").expect("written");
        let used = restore_newest(&paths, &target).expect("restored");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "second", "the copy made before the last writing");
        assert!(!std::path::Path::new(&used).exists(), "the copy that was put back is used up");
        let _ = std::fs::remove_dir_all(home);
    }
}
