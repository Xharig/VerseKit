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

//! What the page "Erkennung" (`_detection` and `_account_rows` of 4.12) needs besides the window: the
//! phrases found, the accounts of the logs, the own account, the blueprints of other accounts that ended
//! up in the collection, and the switch "also blueprints with no known source".
//!
//! The collection is written by the window here (as it does for a tick in the blueprint list); the thread of
//! the overlay takes the file over before it writes its next find (`Run::refresh` of the watcher), so
//! neither takes back what the other did.

use crate::collection::{self, Collection, NoCatalog};
use crate::export::{self, FileExports};
use crate::jsonio::Value;
use crate::logsource::{self, ACCOUNT_ID_SETTING, ACCOUNT_SETTING, ALL_ACCOUNTS, ForeignTally, LogSource};
use crate::overlay::keys;
use crate::paths::Paths;
use crate::phrases::{self, BackRoute, NoArchive};

/// The sources of a blueprint that came from a log; only these are taken out as "foreign".
pub const FROM_LOG: [&str; 2] = ["log", "nachlese"];

/// The phrases the program looks for, as the page shows them (one after the other); empty when none.
pub fn phrases_found(paths: &Paths) -> Vec<String> {
    LogSource::sources_of(paths).collect().0
}

/// The accounts of the logs and the own one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Accounts {
    /// Every account with its number of logs, the own one too (with 0 if it has none), sorted without regard
    /// to case.
    pub list: Vec<(String, usize)>,
    /// What counts: an account, [`ALL_ACCOUNTS`] for all, `None` while none can be told yet.
    pub own: Option<String>,
    /// The blueprints of sessions of another account that did not count.
    pub foreign: ForeignTally,
}

/// Reads the logs (this may take a moment: do it in a thread).
pub fn accounts(paths: &Paths) -> Accounts {
    let mut found = list_accounts(paths);
    found.foreign = logsource::foreign_tally(paths);
    found
}

fn list_accounts(paths: &Paths) -> Accounts {
    let source = LogSource::new(paths, LogSource::sources_of(paths));
    let counts = source.accounts_in_logs(None);
    let own = source.own_account(None);
    let mut list = counts;
    if let Some(own) = own.as_ref().filter(|own| *own != ALL_ACCOUNTS)
        && !list.iter().any(|(name, _)| name == own)
    {
        list.push((own.clone(), 0));
    }
    list.sort_by_cached_key(|(name, _)| name.to_lowercase());
    // The handles of the player are private for the streamer mode.
    for (name, _) in &list {
        crate::streamer::learn(name);
    }
    if let Some(own) = &own {
        crate::streamer::learn(own);
    }
    Accounts { list, own, foreign: ForeignTally::default() }
}

/// Makes `value` the own account (an account, or [`ALL_ACCOUNTS`]) and asks the overlay to read the logs
/// again. False if nothing changed (the same as before) or the settings could not be written. True once the
/// account is saved: if only the request to the overlay fails, the choice still counts (it reads it at its next
/// start), so the page must not look as if nothing happened.
///
/// The account number goes with it (from the newest log of that handle that names one), and the tally of
/// blueprints that did not count is dropped: the overlay reads all logs again and tallies anew.
pub fn set_account(paths: &Paths, value: &str) -> bool {
    let before = paths.setting(ACCOUNT_SETTING).unwrap_or_default();
    if before == value || !paths.set_setting(ACCOUNT_SETTING, Value::str(value)) {
        return false;
    }
    let id = if value == ALL_ACCOUNTS {
        None
    } else {
        LogSource::new(paths, LogSource::sources_of(paths)).account_id_of(value)
    };
    let _ = paths.set_setting(ACCOUNT_ID_SETTING, Value::str(id.unwrap_or_default()));
    let _ = logsource::clear_foreign_tally(paths);
    let _ = ask_for_rescan(paths);
    true
}

/// Raises the number the overlay watches: it reads all logs again.
pub fn ask_for_rescan(paths: &Paths) -> bool {
    let number = paths.setting_int(keys::RESCAN_NR, 0, None, None);
    paths.set_setting(keys::RESCAN_NR, Value::int(number.saturating_add(1)))
}

/// Switches "also blueprints with no known source" (the lists take it up when they load next).
pub fn set_all_blueprints(paths: &Paths, on: bool) -> bool {
    paths.set_setting(export::ALL_SETTING, Value::Bool(on))
}

/// Is it on?
pub fn all_blueprints(paths: &Paths) -> bool {
    paths.setting_bool(export::ALL_SETTING, false)
}

/// The collection as the window reads and writes it.
fn with_collection<R>(paths: &Paths, work: impl FnOnce(&Collection) -> R) -> R {
    let names = BackRoute { paths, archive: &NoArchive };
    let exports = FileExports { paths, version: env!("CARGO_PKG_VERSION"), clock: export::now_unix };
    let collection =
        Collection { paths, catalog: &NoCatalog, names: &names, exports: &exports, clock: collection::now_text };
    work(&collection)
}

/// Blueprints that stand only in logs of other accounts **and** are in the collection from a log
/// (the source `log` or `nachlese`): what the page offers to take out. Reads all logs: do it in a thread.
///
/// ⚠ A blueprint looks foreign here also when the own log with the find is gone already: so this is an offer
/// that is asked about, never done on its own. What the player ticked by hand is not touched.
pub fn foreign(paths: &Paths) -> Vec<String> {
    let source = LogSource::new(paths, LogSource::sources_of(paths));
    let pattern = phrases::pattern(&source.sources.collect().0);
    let names = source.foreign_only_blueprints(&pattern, None);
    let stock = with_collection(paths, |collection| collection.load());
    names.into_iter().filter(|name| came_from_log(&stock, name)).collect()
}

/// Is the blueprint in the collection with a source out of a log?
fn came_from_log(stock: &crate::jsonio::Map, name: &str) -> bool {
    let key = crate::paths::name_key(name);
    collection::blueprints(stock)
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|(_, entry)| entry.get("quelle"))
        .and_then(Value::as_str)
        .is_some_and(|source| FROM_LOG.contains(&source))
}

/// Takes the blueprints out of the collection that came from a log (the others stay, also when named).
/// Gives how many went, or `None` when the collection could not be written.
pub fn remove_foreign(paths: &Paths, names: &[String]) -> Option<usize> {
    with_collection(paths, |collection| {
        // Read, change and write as one step.
        let _lock = crate::jsonio::lock_saves();
        let mut stock = collection.load();
        let doomed: Vec<&String> = names.iter().filter(|name| came_from_log(&stock, name)).collect();
        for name in &doomed {
            collection::remove(&mut stock, name);
        }
        if doomed.is_empty() {
            return Some(0);
        }
        collection.save(&mut stock).then_some(doomed.len())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Host;
    use crate::pypath::Flavor;

    fn scratch(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("versekit-detection-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("folder");
        dir.to_string_lossy().replace('\\', "/")
    }

    fn paths_in(home: &str, extra: &[(&str, &str)]) -> Paths {
        let mut vars = vec![("SC_BP_HOME", home), ("HOME", home)];
        vars.extend_from_slice(extra);
        Paths::new(Host::new(Flavor::Posix, &vars, home))
    }

    /// A game folder with a log of each of two accounts (two backups) that each found a blueprint.
    fn world(name: &str) -> (String, Paths) {
        let home = scratch(name);
        let game = format!("{home}/StarCitizen/LIVE");
        let log = |account: &str, find: &str| {
            format!(
                "<Legacy login response> [CIG-net] User Login Success - Handle[{account}] - Time\n\
                 <SHUDEvent_OnNotification> Added notification \"Received Blueprint: {find}: \" [1]\n"
            )
        };
        let backups = format!("{game}/logbackups");
        std::fs::create_dir_all(&backups).expect("folder");
        std::fs::write(format!("{backups}/1.log"), log("Alice", "Mine Rifle")).unwrap();
        std::fs::write(format!("{backups}/2.log"), log("Alice", "Mine Helmet")).unwrap();
        std::fs::write(format!("{backups}/3.log"), log("Bob", "Other Pistol")).unwrap();
        std::fs::write(format!("{game}/Game.log"), "x\n").unwrap();
        let paths = paths_in(&home, &[(crate::paths::ENV_INSTALL_DIR, game.as_str())]);
        (home, paths)
    }

    #[test]
    fn the_accounts_come_with_their_logs_sorted_and_the_own_one_is_the_one_with_most() {
        let (home, paths) = world("konten");
        let found = accounts(&paths);
        assert_eq!(found.list, [("Alice".to_string(), 2), ("Bob".to_string(), 1)], "{found:?}");
        assert_eq!(found.own.as_deref(), Some("Alice"), "most logs wins");
        assert_eq!(paths.setting(ACCOUNT_SETTING).as_deref(), Some("Alice"), "and is fixed from then on");
        // An own account without a log of its own is on the list all the same, with 0.
        // Sorted without regard to case: a name in small letters stands first, not behind the capitals.
        assert!(paths.set_setting(ACCOUNT_SETTING, Value::str("alex")));
        let found = accounts(&paths);
        assert_eq!(found.own.as_deref(), Some("alex"));
        assert_eq!(
            found.list.iter().map(|(n, c)| (n.as_str(), *c)).collect::<Vec<_>>(),
            [("alex", 0), ("Alice", 2), ("Bob", 1)]
        );
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn choosing_an_account_is_stored_and_asks_the_overlay_once_and_the_same_one_asks_nothing() {
        let home = scratch("waehlen");
        let paths = paths_in(&home, &[]);
        let number = || paths.setting_int(keys::RESCAN_NR, 0, None, None);
        assert_eq!(number(), 0);
        assert!(set_account(&paths, "Bob"));
        assert_eq!(paths.setting(ACCOUNT_SETTING).as_deref(), Some("Bob"));
        assert_eq!(number(), 1, "the overlay is asked to read again");
        assert!(!set_account(&paths, "Bob"), "the same again changes nothing");
        assert_eq!(number(), 1);
        assert!(set_account(&paths, ALL_ACCOUNTS));
        assert_eq!(paths.setting(ACCOUNT_SETTING).as_deref(), Some(ALL_ACCOUNTS));
        assert_eq!(number(), 2);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_switch_for_blueprints_with_no_known_source_is_stored_and_off_at_the_start() {
        let home = scratch("alle");
        let paths = paths_in(&home, &[]);
        assert!(!all_blueprints(&paths));
        assert!(set_all_blueprints(&paths, true));
        assert!(all_blueprints(&paths));
        assert!(set_all_blueprints(&paths, false));
        assert!(!all_blueprints(&paths));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn foreign_blueprints_are_offered_only_when_they_came_from_a_log_and_only_those_go() {
        let (home, paths) = world("fremd");
        assert!(paths.set_setting(ACCOUNT_SETTING, Value::str("Alice")));
        // The collection: the foreign find from a log, a foreign one the player ticked by hand, an own one.
        with_collection(&paths, |collection| {
            let mut stock = collection.load();
            assert!(collection.add(&mut stock, "Other Pistol", "log", None));
            assert!(collection.add(&mut stock, "Mine Rifle", "log", None));
            assert!(collection.save(&mut stock));
        });
        assert_eq!(foreign(&paths), ["Other Pistol"], "the own find is not foreign");
        // The same blueprint ticked by hand: no offer (the source is the player's).
        with_collection(&paths, |collection| {
            let mut stock = collection.load();
            collection::remove(&mut stock, "Other Pistol");
            assert!(collection.add(&mut stock, "Other Pistol", "hand", None));
            assert!(collection.save(&mut stock));
        });
        assert!(foreign(&paths).is_empty(), "{:?}", foreign(&paths));
        // Removing: only what came from a log, and the count says how many.
        with_collection(&paths, |collection| {
            let mut stock = collection.load();
            collection::remove(&mut stock, "Other Pistol");
            assert!(collection.add(&mut stock, "Other Pistol", "nachlese", None));
            assert!(collection.save(&mut stock));
        });
        let names = vec!["Other Pistol".to_string(), "Mine Rifle".to_string(), "Nowhere Boots".to_string()];
        assert_eq!(remove_foreign(&paths, &names), Some(2), "Mine Rifle came from a log too: asked for, it goes");
        let left = with_collection(&paths, |collection| collection::names(&collection.load()));
        assert!(left.is_empty(), "{left:?}");
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn taking_foreign_blueprints_out_waits_for_whoever_writes_the_collection_at_that_moment() {
        let (home, paths) = world("fremd-sperre");
        with_collection(&paths, |collection| {
            let mut stock = collection.load();
            assert!(collection.add(&mut stock, "Other Pistol", "log", None));
            assert!(collection.save(&mut stock));
        });
        // Another writer holds the lock and ticks a blueprint while the removal wants to start. Had the removal
        // read the collection before it got the lock, that blueprint would be gone after it.
        let guard = crate::jsonio::lock_saves();
        let host = paths.host.clone();
        let worker = std::thread::spawn(move || remove_foreign(&Paths::new(host), &["Other Pistol".to_string()]));
        std::thread::sleep(std::time::Duration::from_millis(400));
        with_collection(&paths, |collection| {
            let mut stock = collection.load();
            assert!(collection.add(&mut stock, "Ticked Meanwhile", "hand", None));
            assert!(collection.save(&mut stock));
        });
        drop(guard);
        assert_eq!(worker.join().expect("worker"), Some(1));
        let left = with_collection(&paths, |collection| collection::names(&collection.load()));
        assert_eq!(left, ["Ticked Meanwhile"]);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_blueprint_ticked_by_hand_stays_when_it_is_named_for_removal() {
        let (home, paths) = world("hand");
        with_collection(&paths, |collection| {
            let mut stock = collection.load();
            assert!(collection.add(&mut stock, "Other Pistol", "hand", None));
            assert!(collection.save(&mut stock));
        });
        assert_eq!(remove_foreign(&paths, &["Other Pistol".to_string()]), Some(0));
        let left = with_collection(&paths, |collection| collection::names(&collection.load()));
        assert_eq!(left, ["Other Pistol"]);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_phrases_found_are_the_table_without_a_language_file() {
        let home = scratch("saetze");
        let paths = paths_in(&home, &[]);
        let found = phrases_found(&paths);
        assert!(
            found.iter().any(|p| p == "Bauplan erhalten") && found.iter().any(|p| p == "Received Blueprint"),
            "{found:?}"
        );
        let _ = std::fs::remove_dir_all(home);
    }
}
