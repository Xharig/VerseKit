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

//! The quiet fetches in the background (`_preise_tick` of 4.12): the game
//! build first, so that what is fetched next carries the right stamp, then the
//! raw material prices, the places, the prices per terminal and the ships.
//! Each list fetches only when it is missing or stale (a day, a patch), so a
//! pass over fresh files asks nobody.

use std::time::SystemTime;

use crate::commodity_prices;
use crate::paths::Paths;
use crate::places;
use crate::selling::{self, Update};
use crate::ship_market;
use crate::stock_fetch::{self, Fetch, Outcome};

/// What one pass did.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    /// The game build is known (fetched or remembered).
    pub build: bool,
    pub prices: bool,
    pub places: bool,
    pub selling: Update,
    pub ships: bool,
}

/// One pass over the lists at the time `now` (seconds since 1970).
pub fn pass(paths: &Paths, now: f64, fetch: Fetch<'_>) -> Pass {
    let moment = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs_f64(now);
    let build = matches!(stock_fetch::update_build_with(paths, moment, fetch), Outcome::Fresh | Outcome::Current);
    Pass {
        build,
        prices: commodity_prices::update(paths, now, fetch),
        places: places::update(paths, now, fetch),
        selling: selling::update(paths, now, false, fetch),
        ships: ship_market::update(paths, now, fetch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scmdb::FetchError;

    #[test]
    fn a_pass_without_a_network_changes_nothing_and_does_not_fail() {
        let folder = std::env::temp_dir().join(format!("versekit-sync-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let home = folder.to_string_lossy().into_owned();
        let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &[(crate::legacy::ENV_HOME, &home)], &home);
        let paths = Paths::new(host);
        let asked = std::sync::atomic::AtomicUsize::new(0);
        let fetch = |_: &str| -> Result<Vec<u8>, FetchError> {
            asked.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Err(FetchError::Offline)
        };
        let result = pass(&paths, 1_790_000_000.0, &fetch);
        assert!(!result.prices && !result.places && !result.ships && !result.build);
        assert_eq!(result.selling, Update::Network);
        assert!(asked.load(std::sync::atomic::Ordering::Relaxed) >= 5, "every list was asked once");
        // With the network switched off in the environment given nobody is asked, whatever the shell has.
        let mut off = paths.host.clone();
        off.set_var(crate::legacy::ENV_NO_NET, "1");
        let before = asked.load(std::sync::atomic::Ordering::Relaxed);
        let result = pass(&Paths::new(off), 1_790_000_000.0, &fetch);
        assert_eq!(result.selling, Update::Off);
        assert!(!result.prices && !result.places && !result.ships);
        let lists_asked = asked.load(std::sync::atomic::Ordering::Relaxed) - before;
        assert_eq!(lists_asked, 1, "only the build is asked, by the switch of its function object");
        let files = std::fs::read_dir(&folder).unwrap().count();
        assert_eq!(files, 0, "nothing was written");
        let _ = std::fs::remove_dir_all(folder);
    }
}
