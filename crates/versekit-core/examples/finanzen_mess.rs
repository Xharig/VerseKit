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

//! Times each read the page "Finances" does on its first look, on the data folder of this computer. Reads only.

use std::time::Instant;

use versekit_core::paths::{Host, Paths};
use versekit_core::stock_journal::names::{Items, Places};
use versekit_core::{contract_rewards, finance, mission_log, playtime, stock_journal};

fn timed<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = f();
    println!("{name}: {:.2} s", start.elapsed().as_secs_f64());
    value
}

fn main() {
    let paths = Paths::new(Host::current());
    println!("journal file: {}", paths.app_file(stock_journal::FILE));
    let journal = timed("stock_journal::load", || stock_journal::load(&paths));
    let sessions = timed("stock_journal::sessions", || stock_journal::sessions(&journal));
    let missions = timed("mission_log::load", || mission_log::load(&paths));
    println!("sessions {}, missions {}, ini files {}", sessions.len(), missions.len(), mission_log::ini_files(&paths).len());
    let rewards = timed("contract_rewards::load (first)", || contract_rewards::Rewards::load_kept(&paths));
    println!("rewards empty: {}", rewards.is_empty());
    timed("contract_rewards::load (kept)", || contract_rewards::Rewards::load_kept(&paths));
    timed("playtime::spans", || playtime::spans(&paths));
    timed("finance::counts", || finance::counts(&paths));
    timed("Items::load", || Items::load(&paths));
    timed("Places::load", || Places::load(&paths));
    // The blueprint answer of the overlay and of the contract log: an index over the language files, made at every start.
    timed("mission_log::ContractBlueprints::load", || mission_log::ContractBlueprints::load(&paths));
    timed("mission_log::ContractBlueprints::load (again)", || mission_log::ContractBlueprints::load(&paths));
}
