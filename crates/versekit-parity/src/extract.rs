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

//! `versekit-parity auszug <kanalordner> <ausgabeordner>`: keeps the
//! DataForge (`Game2.dcb`), the English `global.ini` and the p4kx catalog of
//! the contracts (`contracts.json`) of the `Data.p4k` in `<kanalordner>` as
//! files in `<ausgabeordner>`, together with the scmdb build name, so that
//! the slow archive (a network share) is read once and the tests and tools
//! that take `VERSEKIT_DATAFORGE`, `VERSEKIT_GLOBAL_INI` and
//! `VERSEKIT_CONTRACTS` run on the copies. Nothing is written to the game
//! folder, and nothing of it belongs into the repository.

use std::path::Path;

use std::collections::HashMap;

use versekit_gamedata::{
    ContractFacts, GameData, Language, Localization, ShownKeys, blueprints_from_dataforge,
};

/// The file of an extract that holds the p4kx catalog of the contracts.
pub const CONTRACTS_FILE: &str = "contracts.json";

/// The files of an extract with the p4kx catalogs of the faction reputation records and of the reputation records.
pub const REPUTATION_FILES: [(&str, &str); 2] = [
    ("factions.json", "libs/foundry/records/factions/factionreputation/"),
    ("reputation.json", "libs/foundry/records/reputation/"),
];

/// The keys every contract entry of the extract in `folder` shows ([`versekit_gamedata::Catalog::shown_keys`]),
/// read from its DataForge with its English texts.
pub fn shown_keys(folder: &Path) -> Result<HashMap<String, ShownKeys>, String> {
    let read = |name: &str| std::fs::read(folder.join(name)).map_err(|e| format!("{name}: {e}"));
    let texts = Localization::from_files(Language::English, &read("global.ini")?, None).map_err(|e| e.to_string())?;
    let facts = contract_facts(folder)?;
    let catalog = blueprints_from_dataforge(&read("Game2.dcb")?, &facts, &texts, &[], None).map_err(|e| e.to_string())?;
    Ok(catalog.shown_keys)
}

/// The contract facts of the extract in `folder`.
pub fn contract_facts(folder: &Path) -> Result<ContractFacts, String> {
    let bytes = std::fs::read(folder.join(CONTRACTS_FILE))
        .map_err(|e| format!("{CONTRACTS_FILE}: {e} (Auszug mit `versekit-parity auszug` neu ziehen)"))?;
    ContractFacts::from_catalog(&bytes).map_err(|e| format!("{CONTRACTS_FILE}: {e}"))
}

/// Opens `source`: a channel folder of the game, read only while the game is closed, or a copy of a `Data.p4k`
/// outside the game folder, read at once.
fn open(source: &str) -> Result<GameData, String> {
    let path = Path::new(source);
    if !path.is_file() {
        return GameData::open(path).map_err(|e| e.to_string());
    }
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let file = canonical(path);
    let game_folder = versekit_core::paths::global().game_folder();
    if game_folder.is_some_and(|folder| file.starts_with(canonical(Path::new(&folder)))) {
        return Err(format!("{source} liegt im Spielordner: Kanalordner angeben, dann wird auf das Spielende gewartet"));
    }
    GameData::open_file_after(&file, &|| true, versekit_core::archive_lock::POLL).map_err(|e| e.to_string())
}

/// `versekit-parity katalogteil <kanalordner|p4k-kopie> <filter> <datei>`: writes the p4kx catalog of the records
/// whose path matches `<filter>` to `<datei>` and prints how long it took.
pub fn part(channel: &str, filter: &str, out: &str) -> Result<(), String> {
    let start = std::time::Instant::now();
    let game = open(channel)?;
    let bytes = game.catalog_of(filter).map_err(|e| e.to_string())?;
    std::fs::write(out, &bytes).map_err(|e| e.to_string())?;
    println!("{filter}: {} Bytes in {:.1} s", bytes.len(), start.elapsed().as_secs_f64());
    Ok(())
}

pub fn run(channel: &str, out: &str) -> Result<(), String> {
    let game = open(channel)?;
    let out = Path::new(out);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let dcb = game.dataforge_bytes().map_err(|e| e.to_string())?;
    let ini = game.english_ini_bytes().map_err(|e| e.to_string())?;
    let contracts = game.contract_catalog().map_err(|e| e.to_string())?;
    std::fs::write(out.join("Game2.dcb"), &dcb).map_err(|e| e.to_string())?;
    std::fs::write(out.join("global.ini"), &ini).map_err(|e| e.to_string())?;
    std::fs::write(out.join(CONTRACTS_FILE), &contracts).map_err(|e| e.to_string())?;
    for (file, filter) in REPUTATION_FILES {
        let bytes = game.catalog_of(filter).map_err(|e| e.to_string())?;
        std::fs::write(out.join(file), &bytes).map_err(|e| e.to_string())?;
    }
    let build = game.scmdb_build().unwrap_or_default();
    std::fs::write(out.join("build.txt"), format!("{build}\n{}\n", game.version().unwrap_or_default()))
        .map_err(|e| e.to_string())?;
    println!(
        "Game2.dcb {} Bytes, global.ini {} Bytes, {CONTRACTS_FILE} {} Bytes, Stand {build}",
        dcb.len(),
        ini.len(),
        contracts.len()
    );
    Ok(())
}
