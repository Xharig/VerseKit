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

//! Game data read from the `Data.p4k` of a game channel: the texts of a
//! language and the blueprint catalog with the contracts that award each
//! blueprint.
//!
//! This crate writes nothing. The archive is opened read-only with shared
//! access, so the launcher and the game keep reading and writing it.
//! [`GameData::open_installed`] asks `versekit_core::paths` for the game
//! folder, which reads the settings and may write there itself.

pub mod cart_data;
mod catalog;
mod contract_facts;
pub mod contract_info;
pub mod control_profile;
pub mod corrections;
pub mod craft_book;
pub mod cross_check;
mod error;
pub mod farm;
mod fields;
pub mod game_text;
mod item_values;
pub mod list;
mod localization;
pub mod mineables;
pub mod place_names;
pub mod product_table;
pub mod quality;
pub mod quality_goods;
mod recipes;
pub mod scmdb;
pub mod scmdb_catalog;
pub mod ship_slots;
mod sources;
pub mod value_source;
pub mod values;
mod values_read;
pub mod values_store;

use std::io::Read;
use std::path::{Path, PathBuf};

use p4kx_archive::{Archive, Entry};
use p4kx_dataforge::DataForge;

pub use catalog::{Blueprint, Catalog, Item, Manufacturer, Source, SourceKind, Text};
pub use contract_facts::ContractFacts;
pub use error::Error;
pub use localization::{Language, Localization};

/// The name of the archive in a channel folder.
pub use versekit_core::paths::ARCHIVE_NAME;

/// The DataForge entries, in the order they are tried.
const DATAFORGE_ENTRIES: [&str; 2] = ["Data\\Game2.dcb", "Data\\Game.dcb"];

/// The largest entry that is read into memory.
const MAX_ENTRY_LEN: u64 = 2 << 30;

/// The opened `Data.p4k` of one game channel.
#[derive(Debug)]
pub struct GameData {
    archive: Archive,
}

impl GameData {
    /// Opens the `Data.p4k` in `channel_folder`, e.g. the `LIVE` folder.
    ///
    /// # Errors
    ///
    /// [`Error::Archive`] when the file is missing or no readable archive.
    pub fn open(channel_folder: impl AsRef<Path>) -> Result<Self, Error> {
        Self::open_file(channel_folder.as_ref().join(ARCHIVE_NAME))
    }

    /// Opens the archive file `path`, wherever it lies.
    ///
    /// # Errors
    ///
    /// [`Error::Archive`] when the file is missing or no readable archive.
    pub fn open_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let paths = versekit_core::paths::global();
        Self::open_file_after(path, &|| versekit_core::archive_lock::may_read(paths), versekit_core::archive_lock::POLL)
    }

    /// [`GameData::open_file`] with the lock given: waits, asking `closed` every `poll`, until it says that the game is
    /// closed. A thread of its own reads; this call waits without an error.
    ///
    /// # Errors
    ///
    /// [`Error::Archive`] when the file is missing or no readable archive.
    pub fn open_file_after(
        path: impl AsRef<Path>,
        closed: &dyn Fn() -> bool,
        poll: std::time::Duration,
    ) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        // A file that is not there is no reading to wait with.
        if path.is_file() {
            versekit_core::archive_lock::wait_with(closed, poll);
        }
        let archive = Archive::open(&path).map_err(|source| Error::Archive { path, source: Box::new(source) })?;
        Ok(Self { archive })
    }

    /// Opens the `Data.p4k` `versekit_core::paths` finds: the file given from
    /// outside, else the one in the game folder.
    ///
    /// # Errors
    ///
    /// [`Error::NoGameFolder`] when no game folder is found, otherwise the
    /// errors of [`GameData::open_file`].
    pub fn open_installed() -> Result<Self, Error> {
        let file = versekit_core::paths::global().archive_file().ok_or(Error::NoGameFolder)?;
        Self::open_file(file)
    }

    /// The path of the archive file.
    pub fn archive_path(&self) -> &Path {
        self.archive.path()
    }

    /// The channel folder holding the archive.
    pub fn channel_folder(&self) -> PathBuf {
        self.archive.path().parent().map(Path::to_path_buf).unwrap_or_default()
    }

    /// The game version from the channel's `build_manifest.id`, e.g.
    /// `4.10.197.2878`, or `None` when it is unknown.
    pub fn version(&self) -> Option<&str> {
        self.archive.build().map(p4kx_archive::Build::version)
    }

    /// The branch from the channel's `build_manifest.id`, e.g.
    /// `sc-alpha-4.10.2`, or `None` when it is unknown.
    pub fn branch(&self) -> Option<&str> {
        self.archive.build().map(p4kx_archive::Build::branch)
    }

    /// The texts of `language`, with English as fallback for keys the
    /// language lacks.
    ///
    /// # Errors
    ///
    /// [`Error::MissingEntry`] when the archive holds no English
    /// `global.ini`, [`Error::Read`] or [`Error::Localization`] when a file
    /// cannot be read. A missing file of another language leaves only the
    /// English texts.
    pub fn localization(&self, language: Language) -> Result<Localization, Error> {
        let english = self.read_entry(Language::English.global_ini_path())?;
        let own = if language == Language::English {
            None
        } else {
            match self.read_entry(language.global_ini_path()) {
                Ok(bytes) => Some(bytes),
                // An encrypted file of the language counts as missing.
                Err(Error::MissingEntry { .. } | Error::Encrypted { .. }) => None,
                Err(error) => return Err(error),
            }
        };
        Localization::from_files(language, &english, own.as_deref())
    }

    /// The blueprint catalog with the contracts that award each blueprint;
    /// names and descriptions are resolved with `texts`.
    ///
    /// # Errors
    ///
    /// [`Error::MissingEntry`] when the archive holds no DataForge,
    /// [`Error::Encrypted`] when it is encrypted, [`Error::Read`] when it
    /// cannot be read and [`Error::DataForge`] when it is no readable
    /// DataForge.
    pub fn blueprints(&self, texts: &Localization) -> Result<Catalog, Error> {
        self.blueprints_corrected(texts, &[])
    }

    /// Like [`GameData::blueprints`], with the known errors of the data
    /// corrected while reading (see [`corrections::CatalogFixes`]); a
    /// correction made for another build than this archive's is held back.
    ///
    /// # Errors
    ///
    /// The errors of [`GameData::blueprints`].
    pub fn blueprints_corrected(
        &self,
        texts: &Localization,
        corrections: &[corrections::Correction],
    ) -> Result<Catalog, Error> {
        let facts = self.contract_facts()?;
        let (entry, bytes) = self.read_dataforge()?;
        let forge = DataForge::open(&bytes).map_err(|source| Error::DataForge { entry, source: Box::new(source) })?;
        let fixes = corrections::CatalogFixes::new(corrections, self.scmdb_build().as_deref());
        Ok(catalog::read(&forge, texts, fixes, &facts))
    }

    /// What p4kx states for each contract entry of the archive (see
    /// [`ContractFacts`]).
    ///
    /// # Errors
    ///
    /// [`Error::Encrypted`] when the DataForge is encrypted, [`Error::Contracts`]
    /// when p4kx cannot build its catalog of the contracts.
    pub fn contract_facts(&self) -> Result<ContractFacts, Error> {
        ContractFacts::from_catalog(&self.contract_catalog()?)
            .map_err(|source| Error::Contracts { source: Box::new(source) })
    }

    /// The Basetool catalog of p4kx for the contract generators of the
    /// archive, as JSON, e.g. to keep it for tests.
    ///
    /// # Errors
    ///
    /// The errors of [`GameData::contract_facts`].
    pub fn contract_catalog(&self) -> Result<Vec<u8>, Error> {
        if let Some(entry) = DATAFORGE_ENTRIES.iter().find_map(|path| self.find_entry(path))
            && entry.is_encrypted()
        {
            return Err(Error::Encrypted { entry: entry.path().to_string() });
        }
        contract_facts::catalog_json(&self.archive).map_err(|source| Error::Contracts { source: Box::new(source) })
    }

    /// The slots of the player ships and the parts with slots of the archive, in the form of the stored erkul
    /// files ([`ship_slots::SHIPS_FILE`], [`ship_slots::PARTS_FILE`]): p4kx builds its catalog once per filter of
    /// [`ship_slots::FILTERS`].
    ///
    /// # Errors
    ///
    /// [`Error::Encrypted`] for an encrypted DataForge, [`Error::Slots`] when p4kx cannot build a catalog or its
    /// JSON cannot be read.
    pub fn ship_slots(&self) -> Result<(versekit_core::jsonio::Value, versekit_core::jsonio::Value), Error> {
        if let Some(entry) = DATAFORGE_ENTRIES.iter().find_map(|path| self.find_entry(path))
            && entry.is_encrypted()
        {
            return Err(Error::Encrypted { entry: entry.path().to_string() });
        }
        let mut catalogs = ship_slots::Catalogs::default();
        for text in ship_slots::FILTERS {
            let filter = p4kx_export::Filter::new(text);
            let options = p4kx_export::CatalogOptions {
                filter: Some(&filter),
                ..p4kx_export::CatalogOptions::new(std::time::SystemTime::UNIX_EPOCH)
            };
            let built =
                p4kx_export::build_catalog(&self.archive, &options).map_err(|source| Error::Slots { source: Box::new(source) })?;
            let catalog: serde_json::Value =
                serde_json::from_slice(&built.into_parts().0).map_err(|source| Error::Slots { source: Box::new(source) })?;
            catalogs.add(&catalog);
        }
        Ok((ship_slots::ships_file(&catalogs), ship_slots::parts_file(&catalogs)))
    }

    /// The game values of the archive: every recipe with the quality
    /// modifiers of its slots, the item each makes, and every ship weapon,
    /// personal weapon and shield with its base values. English names are
    /// taken from `texts`. Nothing is corrected yet; see
    /// [`corrections::apply`].
    ///
    /// # Errors
    ///
    /// The errors of [`GameData::blueprints`].
    pub fn values(&self, texts: &Localization) -> Result<values::GameValues, Error> {
        let (entry, bytes) = self.read_dataforge()?;
        values_from_dataforge(&bytes, texts, self.scmdb_build()).map_err(|source| match source {
            Error::DataForge { source, .. } => Error::DataForge { entry, source },
            other => other,
        })
    }

    /// The build in the form scmdb names its files, e.g.
    /// `4.10.2-live.12913470`: the version of the branch, the channel and the
    /// change number; `None` when the build is unknown.
    pub fn scmdb_build(&self) -> Option<String> {
        let build = self.archive.build()?;
        let folder = self.channel_folder().file_name()?.to_string_lossy().to_string();
        scmdb_build(build.branch(), channel_of(&folder)?, build.change_number())
    }

    /// The DataForge bytes of the archive, e.g. to keep them for tests.
    ///
    /// # Errors
    ///
    /// The errors of [`GameData::blueprints`] for reading the entry.
    pub fn dataforge_bytes(&self) -> Result<Vec<u8>, Error> {
        self.read_dataforge().map(|(_, bytes)| bytes)
    }

    /// The bytes of the English `global.ini`, e.g. to keep them for tests.
    ///
    /// # Errors
    ///
    /// The errors of [`GameData::localization`] for reading the entry.
    pub fn english_ini_bytes(&self) -> Result<Vec<u8>, Error> {
        self.read_entry(Language::English.global_ini_path())
    }

    /// The bytes of an entry of the archive, e.g. the `defaultProfile.xml`.
    ///
    /// # Errors
    ///
    /// [`Error::MissingEntry`], [`Error::Encrypted`], [`Error::TooLarge`] or [`Error::Read`].
    pub fn entry_bytes(&self, path: &str) -> Result<Vec<u8>, Error> {
        self.read_entry(path)
    }

    fn read_dataforge(&self) -> Result<(String, Vec<u8>), Error> {
        for path in DATAFORGE_ENTRIES {
            match self.read_entry(path) {
                Ok(bytes) => return Ok((path.to_string(), bytes)),
                Err(Error::MissingEntry { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        Err(Error::MissingEntry { entry: DATAFORGE_ENTRIES[0].to_string() })
    }

    fn find_entry(&self, path: &str) -> Option<&Entry> {
        self.archive.entries().iter().find(|entry| same_entry_path(entry.path(), path))
    }

    fn read_entry(&self, path: &str) -> Result<Vec<u8>, Error> {
        let entry = self.find_entry(path).ok_or_else(|| Error::MissingEntry { entry: path.to_string() })?;
        // An encrypted entry is never opened: VerseKit has no key and asks
        // for none (Game2.dcb and global.ini are not encrypted today).
        if entry.is_encrypted() {
            return Err(Error::Encrypted { entry: path.to_string() });
        }
        let size = entry.uncompressed_size();
        if size > MAX_ENTRY_LEN {
            return Err(Error::TooLarge { entry: path.to_string(), size });
        }
        let read_error = |source| Error::Read { entry: path.to_string(), source };
        let mut reader = self.archive.open_entry(entry).map_err(|source| {
            if source.is_key_required() {
                Error::Encrypted { entry: path.to_string() }
            } else {
                Error::Archive { path: self.archive.path().to_path_buf(), source: Box::new(source) }
            }
        })?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(usize::try_from(size).unwrap_or(usize::MAX))
            .map_err(|_| Error::TooLarge { entry: path.to_string(), size })?;
        reader.read_to_end(&mut bytes).map_err(read_error)?;
        Ok(bytes)
    }
}

/// Notes in the error log that the game data of `channel` are encrypted
/// and stay unread; for any other error nothing is noted and `false` is
/// returned. VerseKit keeps the values it read last (or takes scmdb's files,
/// see [`value_source`]) and never asks the user for a key.
pub fn record_encrypted(log: &versekit_core::error_log::ErrorLog<'_>, channel: &str, error: &Error) -> bool {
    let Error::Encrypted { entry } = error else { return false };
    let note = format!("{} ({channel}: {entry})", versekit_core::language::t("s_p3_verschluesselt"));
    log.record("gamedata.encrypted", None, &note)
}

/// The blueprint catalog of the DataForge `bytes` (see [`GameData::blueprints`]),
/// e.g. of a copy kept for tests; `facts` are those of the same archive
/// ([`GameData::contract_facts`], or [`ContractFacts::from_catalog`] of the
/// kept [`GameData::contract_catalog`]).
///
/// `corrections` apply to `build` (see [`GameData::blueprints_corrected`]).
///
/// # Errors
///
/// [`Error::DataForge`] when the bytes are no readable DataForge.
pub fn blueprints_from_dataforge(
    bytes: &[u8],
    facts: &ContractFacts,
    texts: &Localization,
    corrections: &[corrections::Correction],
    build: Option<&str>,
) -> Result<Catalog, Error> {
    let forge = DataForge::open(bytes)
        .map_err(|source| Error::DataForge { entry: DATAFORGE_ENTRIES[0].to_string(), source: Box::new(source) })?;
    Ok(catalog::read(&forge, texts, corrections::CatalogFixes::new(corrections, build), facts))
}

/// The game values of the DataForge `bytes` (see [`GameData::values`]).
///
/// # Errors
///
/// [`Error::DataForge`] when the bytes are no readable DataForge.
pub fn values_from_dataforge(
    bytes: &[u8],
    texts: &Localization,
    build: Option<String>,
) -> Result<values::GameValues, Error> {
    let forge = DataForge::open(bytes)
        .map_err(|source| Error::DataForge { entry: DATAFORGE_ENTRIES[0].to_string(), source: Box::new(source) })?;
    let mut values = values_read::read(&forge, texts, build);
    values.crafter_limits = mineables::crafter_limits(&mineables::crafters(&forge));
    Ok(values)
}

/// The game channels, as their folders end and as scmdb names them.
const CHANNELS: [(&str, &str); 5] =
    [("TECH-PREVIEW", "tech-preview"), ("EPTU", "eptu"), ("PTU", "ptu"), ("HOTFIX", "hotfix"), ("LIVE", "live")];

/// The channel of a channel folder by the end of its name (`LIVE`, also a
/// copy such as `Spieldaten LIVE`), `None` for any other folder.
fn channel_of(folder: &str) -> Option<&'static str> {
    let upper = folder.trim().to_ascii_uppercase().replace('_', "-");
    CHANNELS
        .iter()
        .find(|(end, _)| {
            upper == *end
                || upper.strip_suffix(end).is_some_and(|rest| rest.ends_with([' ', '-']) && !rest.ends_with("E-"))
        })
        .map(|(_, name)| *name)
}

/// The scmdb build name of a branch such as `sc-alpha-4.10.2`, a channel
/// such as `live` and a change number.
fn scmdb_build(branch: &str, channel: &str, change: &str) -> Option<String> {
    let version = branch.rsplit('-').next().filter(|v| v.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
    let digits = !change.is_empty() && change.bytes().all(|b| b.is_ascii_digit());
    (digits && !channel.is_empty()).then(|| format!("{version}-{channel}.{change}"))
}

/// Compares two entry paths ignoring ASCII case and the kind of separator.
fn same_entry_path(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes().zip(b.bytes()).all(|(x, y)| {
            let x = if x == b'/' { b'\\' } else { x.to_ascii_lowercase() };
            let y = if y == b'/' { b'\\' } else { y.to_ascii_lowercase() };
            x == y
        })
}

#[cfg(test)]
mod tests {
    use super::{channel_of, same_entry_path, scmdb_build};

    #[test]
    fn the_channel_is_read_from_the_end_of_the_folder_name() {
        assert_eq!(channel_of("LIVE"), Some("live"));
        assert_eq!(channel_of("Spieldaten LIVE"), Some("live"));
        assert_eq!(channel_of("PTU"), Some("ptu"));
        assert_eq!(channel_of("EPTU"), Some("eptu"));
        assert_eq!(channel_of("TECH-PREVIEW"), Some("tech-preview"));
        assert_eq!(channel_of("OLIVE"), None);
        assert_eq!(channel_of("Daten"), None);
    }

    #[test]
    fn the_scmdb_build_is_branch_version_channel_and_change() {
        assert_eq!(scmdb_build("sc-alpha-4.10.2", "live", "12913470").as_deref(), Some("4.10.2-live.12913470"));
        assert_eq!(scmdb_build("sc-alpha-4.11.0", "ptu", "13000000").as_deref(), Some("4.11.0-ptu.13000000"));
        assert_eq!(scmdb_build("sc-alpha-4.10.2", "live", ""), None);
        assert_eq!(scmdb_build("main", "live", "1"), None);
    }

    #[test]
    fn the_archive_is_opened_only_after_the_game_is_closed_and_a_missing_file_does_not_wait() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::{Duration, Instant};

        let dir = std::env::temp_dir().join(format!("versekit-sperre-p4k-{}", versekit_core::testdir::run_tag()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Data.p4k");
        std::fs::write(&file, b"no archive").unwrap();
        let closed = Arc::new(AtomicBool::new(false));
        let reader = {
            let (closed, file) = (Arc::clone(&closed), file.clone());
            std::thread::spawn(move || {
                let started = Instant::now();
                let outcome = super::GameData::open_file_after(
                    &file,
                    &|| closed.load(Ordering::SeqCst),
                    Duration::from_millis(5),
                );
                (outcome.is_err(), started.elapsed())
            })
        };
        let waited = Instant::now();
        while !versekit_core::archive_lock::waiting() && waited.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(versekit_core::archive_lock::waiting(), "the reading waits while the game runs");
        std::thread::sleep(Duration::from_millis(60));
        assert!(!reader.is_finished(), "and does not read");
        closed.store(true, Ordering::SeqCst);
        let (failed, took) = reader.join().expect("the reading ended");
        assert!(failed, "the file is no archive, which is said only now");
        assert!(took >= Duration::from_millis(60), "{took:?}");
        // A file that is not there fails at once, whatever the game does.
        let started = Instant::now();
        let gone = super::GameData::open_file_after(dir.join("none.p4k"), &|| false, Duration::from_millis(5));
        assert!(gone.is_err() && started.elapsed() < Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn entry_paths_match_ignoring_case_and_separator() {
        assert!(same_entry_path("Data\\Game2.dcb", "data/game2.DCB"));
        assert!(!same_entry_path("Data\\Game2.dcb", "Data\\Game.dcb"));
        assert!(!same_entry_path("Data\\Game2.dcb", "Data\\Game2.dcbx"));
    }
}
