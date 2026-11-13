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

//! The cargo grids of the ships in the default loadout, port by port: where does the cargo of a ship come from,
//! and which ports hold something that looks like cargo (a module, a container) but is not counted. Prints one
//! block per ship to the file `VERSEKIT_CARGO_OUT` (default: the output of the test).
//!
//! Run with `VERSEKIT_P4K=<Data.p4k> VERSEKIT_CARGO_FILTER=<part of the record path, default every ship> cargo test
//! -p versekit-gamedata --release --test ship_cargo_real -- --ignored --nocapture --test-threads=2`. Reads the
//! archive, writes nothing in it.

use p4kx_archive::Archive;
use p4kx_export::{Filter, ShipBaseOptions, build_ship_base};

#[test]
#[ignore = "reads the Data.p4k of the installed game"]
fn the_cargo_grids_of_the_ships_port_by_port() {
    let path = std::env::var("VERSEKIT_P4K").expect("VERSEKIT_P4K names no Data.p4k");
    let archive = Archive::open(std::path::Path::new(&path)).expect("a readable Data.p4k");
    let filter = std::env::var("VERSEKIT_CARGO_FILTER").ok().map(|text| Filter::new(&text));
    let mut options = ShipBaseOptions::new(std::time::SystemTime::UNIX_EPOCH);
    options.filter = filter.as_ref();
    let built = build_ship_base(&archive, &options).expect("the ships");
    let mut out = String::new();
    for ship in built.ships() {
        out.push_str(&format!(
            "== {} cargoScu {:?} gridsWithIssue {}\n",
            ship.class_name, ship.cargo.scu, ship.cargo.grids_with_issue
        ));
        for grid in &ship.cargo.grids {
            out.push_str(&format!(
                "   grid {} {} scu {:?} shape {:?} issue {:?}\n",
                grid.port, grid.class_name, grid.scu, grid.shape, grid.issue
            ));
        }
        // Ports that look like a place for cargo but hold no grid the sum counts.
        for port in &ship.loadout {
            let text = format!("{} {}", port.port, port.class_name.as_deref().unwrap_or_default()).to_ascii_lowercase();
            if ["cargo", "module", "container", "pod", "inventory"].iter().any(|word| text.contains(word)) {
                out.push_str(&format!("   port {} {:?} ({})\n", port.port, port.class_name, port.origin));
            }
        }
    }
    println!("{} ships, {} failures", built.ships().len(), built.failures().len());
    match std::env::var("VERSEKIT_CARGO_OUT") {
        Ok(file) => std::fs::write(file, out).expect("the output file"),
        Err(_) => println!("{out}"),
    }
}
