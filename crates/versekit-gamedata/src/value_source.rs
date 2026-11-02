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

//! Where the game values come from: the one place that chooses.
//!
//! As decided on 22.10.:
//!
//! | Source | When | What for |
//! |---|---|---|
//! | [`Source::Archive`], the user's `Data.p4k` | always first; offline, right after a patch | the values VerseKit shows |
//! | [`Source::Scmdb`], fetched at run time | when the own reading fails (archive missing, entry encrypted, unknown format); otherwise beside it | fallback, and the standing cross-check at the same build |
//! | Basetool (basetool#2900, later) | only to compare | the same values and identifiers as the Basetool |
//!
//! scmdb is never shipped and never left out: every successful own reading
//! is compared with scmdb's files of the same build where they can be
//! fetched, and differences go to the error log. Corrections (basetool#2899)
//! apply to whichever source was used (see [`crate::corrections`]).

use std::fmt;
use std::path::Path;

use versekit_core::error_log::ErrorLog;
use versekit_core::scmdb::FetchError;

use crate::cross_check::{self, CrossCheck};
use crate::values::GameValues;
use crate::{Error, GameData, Language, record_encrypted, scmdb};

/// A source of game values, in the order they are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    /// The `Data.p4k` of the user's game channel, read by VerseKit itself.
    Archive,
    /// The scmdb.net files of the same build, fetched at run time.
    Scmdb,
}

impl Source {
    /// The sources in the order of precedence.
    pub const PRECEDENCE: [Self; 2] = [Self::Archive, Self::Scmdb];

    /// The text key naming the source for the user.
    pub const fn text_key(self) -> &'static str {
        match self {
            Self::Archive => "s_p3_quelle_archiv",
            Self::Scmdb => "s_p3_quelle_scmdb_werte",
        }
    }
}

/// The game values with where they come from.
#[derive(Debug)]
pub struct Loaded {
    /// The values, not corrected yet.
    pub values: GameValues,
    /// The source used.
    pub source: Source,
    /// Why the own reading failed, when scmdb stands in.
    pub archive_error: Option<Error>,
    /// The comparison with scmdb at the same build, when the own reading
    /// worked and scmdb's files could be fetched.
    pub cross_check: Option<CrossCheck>,
    /// Why scmdb's files could not be had, when they could not.
    pub scmdb_error: Option<String>,
}

/// Neither source gave values.
#[derive(Debug)]
pub struct NoValues {
    /// Why the own reading failed.
    pub archive: Error,
    /// Why scmdb's files could not be had.
    pub scmdb: String,
}

impl fmt::Display for NoValues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no game values: {}; scmdb: {}", self.archive, self.scmdb)
    }
}

impl std::error::Error for NoValues {}

/// Chooses the source: the own reading `archive` first, then scmdb, which
/// `scmdb` fetches; with an own reading scmdb is fetched as well, for the
/// cross-check.
///
/// # Errors
///
/// [`NoValues`] when neither source gives values.
pub fn choose(
    archive: Result<GameValues, Error>,
    scmdb: impl FnOnce() -> Result<GameValues, String>,
) -> Result<Loaded, NoValues> {
    match archive {
        Ok(mut values) => {
            let (cross_check, scmdb_error) = match scmdb() {
                Ok(theirs) => {
                    let check = cross_check::compare(&values, &theirs);
                    fill_gaps(&mut values, &theirs);
                    (Some(check), None)
                }
                Err(error) => (None, Some(error)),
            };
            Ok(Loaded { values, source: Source::Archive, archive_error: None, cross_check, scmdb_error })
        }
        Err(archive) => match scmdb() {
            Ok(values) => Ok(Loaded {
                values,
                source: Source::Scmdb,
                archive_error: Some(archive),
                cross_check: None,
                scmdb_error: None,
            }),
            Err(scmdb) => Err(NoValues { archive, scmdb }),
        },
    }
}

/// Fills what the own reading does not read from scmdb's values of the same
/// build: the stats of coolers, power plants, quantum drives, radars and the
/// like, armour values, the extra numbers of the product table. What the own
/// reading has stays as it is.
pub fn fill_gaps(values: &mut GameValues, theirs: &GameValues) {
    for (guid, item) in &mut values.items {
        let Some(other) = theirs.items.get(guid) else { continue };
        for (stat, value) in &other.stats {
            item.stats.entry(*stat).or_insert(*value);
        }
        if item.armor.is_none() {
            item.armor.clone_from(&other.armor);
        }
        if item.beams.is_empty() {
            item.beams.clone_from(&other.beams);
        }
        for (name, value) in &other.extra.numbers {
            item.extra.numbers.entry(name.clone()).or_insert(*value);
        }
        for (name, value) in &other.extra.texts {
            item.extra.texts.entry(name.clone()).or_insert_with(|| value.clone());
        }
        for (name, value) in &other.extra.modifiers {
            item.extra.modifiers.entry(name.clone()).or_insert(*value);
        }
        if item.extra.beams.is_empty() {
            item.extra.beams.clone_from(&other.extra.beams);
        }
    }
}

/// The game values of the channel in `channel_folder`, from the source
/// [`choose`] picks; scmdb's files are fetched for the build of the channel,
/// else for scmdb's current LIVE build. `user_agent` names VerseKit
/// (`basetool::config::user_agent`).
///
/// # Errors
///
/// [`NoValues`] when neither source gives values.
pub fn load(channel_folder: impl AsRef<Path>, user_agent: &str) -> Result<Loaded, NoValues> {
    let game = GameData::open(channel_folder);
    let build = game.as_ref().ok().and_then(GameData::scmdb_build);
    let archive = game.and_then(|game| game.localization(Language::English).and_then(|texts| game.values(&texts)));
    // The cross-check needs scmdb's files of the same build; only as a
    // fallback scmdb's current LIVE build stands in for an unknown one.
    let read = archive.is_ok();
    choose(archive, || {
        let build = match build {
            Some(build) => build,
            None if read => return Err("the build of the channel is unknown".to_string()),
            None => scmdb::current_build(user_agent).map_err(|error| error.to_string())?,
        };
        scmdb::fetch_values(&build, user_agent).map_err(|error: FetchError| error.to_string())
    })
}

/// Notes in the error log what the user should know about the source:
/// scmdb standing in for the own reading (and why), an encrypted channel,
/// and differences between the own values and scmdb's at the same build.
/// Returns how many notes were written.
pub fn record(log: &ErrorLog<'_>, channel: &str, loaded: &Loaded) -> usize {
    let t = versekit_core::language::t;
    let mut written = 0;
    if let Some(error) = &loaded.archive_error {
        written += usize::from(record_encrypted(log, channel, error));
        let note = format!("{} ({channel}: {error})", t("s_p3_rueckfall"));
        written += usize::from(log.record("gamedata.fallback", None, &note));
    }
    if let Some(check) = loaded.cross_check.as_ref().filter(|check| check.differences() > 0) {
        let build = loaded.values.build.as_deref().unwrap_or("?");
        let note = format!(
            "{} ({channel}, {build}: {} Abweichungen; {})",
            t("s_p3_gegenprobe"),
            check.differences(),
            check.summary()
        );
        written += usize::from(log.record("gamedata.crosscheck", None, &note));
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::Recipe;

    fn values(tags: &[&str]) -> GameValues {
        GameValues {
            build: Some("4.10.2-live.1".into()),
            recipes: tags
                .iter()
                .map(|tag| Recipe { guid: tag.to_ascii_lowercase(), tag: (*tag).into(), ..Recipe::default() })
                .collect(),
            ..GameValues::default()
        }
    }

    #[test]
    fn the_archive_comes_first_and_scmdb_checks_it() {
        let loaded = choose(Ok(values(&["A", "B"])), || Ok(values(&["A", "C"]))).unwrap();
        assert_eq!(loaded.source, Source::Archive);
        let check = loaded.cross_check.unwrap();
        assert_eq!(check.only_scmdb, ["C"]);
        assert_eq!(check.only_own, ["B"]);
        assert_eq!(check.differences(), 2);
    }

    #[test]
    fn scmdb_stands_in_when_the_archive_fails() {
        let error = Error::Encrypted { entry: "Data\\Game2.dcb".into() };
        let loaded = choose(Err(error), || Ok(values(&["A"]))).unwrap();
        assert_eq!(loaded.source, Source::Scmdb);
        assert!(loaded.archive_error.as_ref().is_some_and(Error::is_encrypted));
        assert!(loaded.cross_check.is_none());
    }

    #[test]
    fn without_scmdb_the_archive_still_counts() {
        let loaded = choose(Ok(values(&["A"])), || Err("offline".into())).unwrap();
        assert_eq!(loaded.source, Source::Archive);
        assert_eq!(loaded.scmdb_error.as_deref(), Some("offline"));
    }

    #[test]
    fn without_either_there_are_no_values() {
        let error = choose(Err(Error::NoGameFolder), || Err("offline".into())).unwrap_err();
        assert_eq!(error.scmdb, "offline");
    }

    #[test]
    fn fallback_and_differences_are_noted_in_the_error_log() {
        use versekit_core::paths::{Host, Paths};
        use versekit_core::pypath::Flavor;
        let home = std::env::temp_dir().join(format!("versekit-value-source-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let dir = home.to_string_lossy().to_string();
        let vars = [("SC_BP_HOME", dir.as_str()), ("HOME", dir.as_str()), ("USERPROFILE", dir.as_str())];
        let paths = Paths::new(Host::new(Flavor::native(), &vars, "/"));
        let log = ErrorLog::new(&paths, "5.0.0-test");
        let same = choose(Ok(values(&["A"])), || Ok(values(&["A"]))).unwrap();
        assert_eq!(record(&log, "LIVE", &same), 0);
        let differ = choose(Ok(values(&["A", "B"])), || Ok(values(&["A"]))).unwrap();
        assert_eq!(record(&log, "LIVE", &differ), 1);
        let fallback =
            choose(Err(Error::Encrypted { entry: "Data\\Game2.dcb".into() }), || Ok(values(&["A"]))).unwrap();
        // Encrypted and fallback: two notes.
        assert_eq!(record(&log, "LIVE", &fallback), 2);
        let notes: Vec<String> = log
            .latest(5)
            .iter()
            .filter_map(|entry| entry.get("stelle").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        assert_eq!(notes.len(), 3, "{notes:?}");
        for place in ["gamedata.crosscheck", "gamedata.fallback", "gamedata.encrypted"] {
            assert!(notes.iter().any(|note| note == place), "{place} missing in {notes:?}");
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_archive_is_first_in_the_precedence() {
        assert_eq!(Source::PRECEDENCE, [Source::Archive, Source::Scmdb]);
        for source in Source::PRECEDENCE {
            for lang in [versekit_core::language::Lang::De, versekit_core::language::Lang::En] {
                assert!(versekit_core::language::text_in(lang, source.text_key()).is_some(), "{source:?}");
            }
        }
    }
}
