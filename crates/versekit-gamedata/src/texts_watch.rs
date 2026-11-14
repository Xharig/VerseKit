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

//! The background of the game's texts as the program runs it: the watch of `versekit_core::game_texts::watch` with
//! the player's `Data.p4k` ([`GameArchive`]), scmdb's recipe file and the builds of the catalog of this crate. The
//! overlay keeps it running; the check without network (`versekit-parity ohne_netz`) runs the same one.

use std::sync::Arc;

use versekit_core::game_texts::watch::{CatalogBuild, CatalogBuilt, CatalogCheck, CraftCheck, Watch};
use versekit_core::game_texts::{Archive, ChannelData, Placed};
use versekit_core::language::{self, Arg, t, tf};
use versekit_core::net::Net;
use versekit_core::paths::Paths;
use versekit_core::translation;

use crate::{game_text, list, reputation_ranks, values_store};

/// The `Data.p4k` of the player, read through this crate.
pub struct GameArchive;

impl Archive for GameArchive {
    fn stamp(&self, paths: &Paths) -> Option<String> {
        game_text::archive_stamp(&paths.game_folder()?)
    }
    fn refresh_original(&self, paths: &Paths) -> bool {
        game_text::refresh_original(paths, None)
    }
    fn fill_gaps(&self, paths: &Paths, target: &str) -> usize {
        game_text::fill_gaps(paths, target, None)
    }
    fn fetch_original(&self, paths: &Paths, root: &str, set_language: bool) -> Result<Placed, String> {
        let set = |language: &str| translation::set_user_cfg(paths, root, language, None);
        match game_text::fetch_original(paths, "english", Some(root), set_language, &set) {
            Ok(game_text::FetchOutcome::Written { bytes }) => Ok(Placed { said: megabytes(bytes), written: true }),
            Err(game_text::FetchError::Read(error)) => Err(error.text_key().map_or_else(|| error.to_string(), t)),
            Err(game_text::FetchError::UnknownTarget) => Err(t("s_tq_kein_spiel")),
            Err(other) => Err(other.to_string()),
        }
    }
    fn refresh_names(&self, paths: &Paths) {
        match paths.channel_root() {
            Some(root) => game_text::names_or_fetch_channel(paths, &root),
            None => game_text::names_or_fetch(paths, None),
        };
    }
    /// The catalog (with reputation and terms) and the values of the channel, read from its own archive without
    /// the net into its data folder, when they do not fit that archive as it is now.
    fn prepare_channel(&self, own: &Paths) -> ChannelData {
        let (Some(root), Some(archive)) = (own.channel_root(), own.archive_file()) else { return ChannelData::Missing };
        let archive = std::path::Path::new(&archive);
        if !archive.is_file() {
            return ChannelData::Missing;
        }
        let cache = own.app_file(list::CACHE_FILE);
        let readable = || versekit_core::archive_lock::may_read_archive(own, archive);
        let key = failed_key(&cache, archive);
        if list::catalog_behind_file(Some(archive), &cache) {
            if CATALOG_FAILED.lock().unwrap_or_else(|e| e.into_inner()).contains(&key) {
                return ChannelData::Missing;
            }
            if !readable() {
                return ChannelData::Later;
            }
            list::rebuild_now_archive(own, Some(archive), &cache, Arc::new(list::Offline)).run();
            if list::installed_build(own).is_none() {
                CATALOG_FAILED.lock().unwrap_or_else(|e| e.into_inner()).push(key);
            }
        }
        let values = values_store::start(
            Some(std::path::Path::new(&root)),
            &own.app_file(values_store::CACHE_FILE),
            Arc::new(values_store::Offline),
        );
        if let Some(rebuild) = values.rebuild.filter(|_| readable()) {
            rebuild.run();
        }
        list::installed_build(own).map_or(ChannelData::Missing, ChannelData::Ready)
    }
    fn refresh_ranks(&self, paths: &Paths, game_version: &str) {
        reputation_ranks::refresh(paths, game_version);
    }
    /// The recipes of the values read from the archive to read (the one given, else the installed one) as it is now
    /// ([`values_store::kept_for_archive`]).
    fn recipes(&self, paths: &Paths) -> Option<versekit_core::jsonio::Value> {
        let archive = paths.archive_file()?;
        let folder = std::path::Path::new(&archive).parent()?;
        let values = values_store::kept_for_archive(folder, &paths.app_file(values_store::CACHE_FILE))?;
        (!values.recipes.is_empty()).then(|| values_store::recipe_file(&values))
    }
}

/// The archive states of the side channels whose catalog could not be built in this run: not tried again for the
/// same state, or every look would read the archive anew.
static CATALOG_FAILED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn failed_key(cache: &str, archive: &std::path::Path) -> String {
    let stamp = std::fs::metadata(archive)
        .ok()
        .map(|meta| (meta.len(), meta.modified().ok()))
        .map_or_else(String::new, |(len, modified)| format!("{len}:{modified:?}"));
    format!("{cache}|{stamp}")
}

fn megabytes(bytes: usize) -> String {
    tf("s_tq_mb", &[("0", Arg::from(language::py_fixed(bytes as f64 / 1_048_576.0, 1, false)))])
}

fn built(catalog: &versekit_core::catalog_cache::CatalogCache) -> CatalogBuilt {
    CatalogBuilt { version: catalog.version.clone(), blueprints: catalog.blueprints.len() }
}

/// The build of the game the notes are written for: the one the catalog was built from when it is current for the
/// installed archive (the launcher's name, known without the network), else the live build UEX named last.
pub fn game_version(paths: &Paths) -> String {
    list::installed_build(paths).unwrap_or_else(|| versekit_core::stock_fetch::live_build(paths))
}

/// The watch of the game's texts that fetches over `net`: after a patch, and when scmdb has new craft data for the
/// catalog, the catalog of the contract data is built anew; a catalog that lags behind the game data at the start (an
/// update of VerseKit with a new layout, a patch while VerseKit was closed) is brought up to date as soon as the game
/// is closed, without a page open. The notes are written for the build of [`game_version`].
pub fn watch(net: Arc<dyn Net>) -> Watch {
    let user_agent = versekit_core::basetool::config::user_agent(env!("CARGO_PKG_VERSION"));
    let recipes = Arc::new(versekit_core::recipe_file::Scmdb { user_agent });
    let catalog: CatalogBuild = Arc::new(|paths: &Paths| list::rebuild_now(paths).run().as_ref().map(built));
    let behind: CraftCheck = Arc::new(|paths: &Paths, build: &str| list::craft_data_behind(paths, build));
    let lags: CatalogCheck = Arc::new(|paths: &Paths| list::catalog_behind(paths));
    let refresh: CatalogBuild = Arc::new(|paths: &Paths| list::refresh_catalog(paths).as_ref().map(built));
    Watch::new(net, Arc::new(GameArchive), recipes)
        .with_catalog(catalog)
        .with_craft_check(behind)
        .with_catalog_refresh(lags, refresh)
        .with_game_version(Arc::new(game_version))
}

#[cfg(test)]
mod tests {
    use versekit_core::paths::{ENV_TEST_MACHINE, Host};
    use versekit_core::pypath::Flavor;

    use super::*;

    /// A side channel whose archive gives no catalog has no data: nothing of the main installation is taken for it,
    /// nothing lands beside the files of the main installation, and the failed build is not tried again at every look.
    #[test]
    fn a_channel_without_a_readable_archive_has_no_data_and_leaves_the_main_files_alone() {
        let dir = std::env::temp_dir().join(format!("versekit-kanal-daten-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.to_string_lossy().into_owned();
        let ptu = format!("{root}/StarCitizen/PTU");
        std::fs::create_dir_all(&ptu).unwrap();
        let data = format!("{root}/data");
        std::fs::create_dir_all(&data).unwrap();
        let vars = [(versekit_core::legacy::ENV_HOME, data.as_str()), (ENV_TEST_MACHINE, "1")];
        let paths = Paths::new(Host::new(Flavor::native(), &vars, &root));
        let main_catalog = paths.app_file(list::CACHE_FILE);
        std::fs::write(&main_catalog, "main").unwrap();
        let own = paths.for_channel("PTU", &ptu);
        assert_eq!(GameArchive.prepare_channel(&own), ChannelData::Missing, "no archive at all");
        std::fs::write(format!("{ptu}/Data.p4k"), b"not an archive").unwrap();
        assert_eq!(GameArchive.prepare_channel(&own), ChannelData::Missing, "an archive that gives no catalog");
        assert_eq!(GameArchive.prepare_channel(&own), ChannelData::Missing, "and no second reading of it");
        assert_eq!(std::fs::read_to_string(&main_catalog).unwrap(), "main");
        let beside: Vec<String> = std::fs::read_dir(&data)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "Kanaele")
            .collect();
        assert_eq!(beside, [list::CACHE_FILE], "the main folder holds its catalog and nothing else new");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
