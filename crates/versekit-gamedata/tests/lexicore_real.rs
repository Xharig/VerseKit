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

//! The import of a real Lexicore file against a copy of the catalog, read only. Not part of the normal run: it
//! needs two paths from the environment.
//!
//! * `LEXICORE_FILE`: the backup (or `owned/LIVE.json`) of Lexicore.
//! * `LEXICORE_DATA`: a folder with a copy of `katalog-spieldaten.json`; it becomes the data folder of the run,
//!   so no live data is touched and no archive is opened.
//!
//! ```text
//! cargo test --locked -p versekit-gamedata --test lexicore_real -- --ignored --nocapture
//! ```

use versekit_core::importer;
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;

#[test]
#[ignore = "needs a real Lexicore file and a copy of the catalog"]
fn a_real_file_of_lexicore_against_a_copy_of_the_catalog() {
    let (Ok(file), Ok(data)) = (std::env::var("LEXICORE_FILE"), std::env::var("LEXICORE_DATA")) else {
        panic!("LEXICORE_FILE and LEXICORE_DATA are needed");
    };
    let data = data.replace('\\', "/");
    let host = Host::new(
        Flavor::native(),
        &[(versekit_core::envvars::ENV_HOME, data.as_str()), (versekit_core::envvars::ENV_NO_NET, "1")],
        &data,
    );
    let paths = Paths::new(host);
    let sources = versekit_core::logsource::LogSource::sources_of(&paths);
    let (kind, entries) = importer::read(&file, &|key| sources.resolve_key(key));
    println!("format: {kind:?}, entries: {}", entries.len());
    let catalog = versekit_gamedata::list::list_catalog_offline(&paths).expect("the copy of the catalog");
    let names: Vec<String> = catalog.blueprints.iter().map(|b| b.name.clone()).collect();
    println!("catalog: {} blueprints", names.len());
    let view = importer::preview(&entries, &Default::default(), &names);
    println!(
        "preview: total {}, new {}, present {}, unknown {}",
        view.total,
        view.new.len(),
        view.present.len(),
        view.unknown.len()
    );
    let dated = entries.iter().filter(|e| e.time.is_some()).count();
    println!("with a time from the history: {dated}");
    for name in &view.unknown {
        println!("unknown: {name}");
    }
}
