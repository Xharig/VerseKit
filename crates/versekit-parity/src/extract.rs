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

//! `versekit-parity auszug <kanalordner> <ausgabeordner>`: keeps the
//! DataForge (`Game2.dcb`), the English `global.ini` and the p4kx catalog of
//! the contracts (`contracts.json`) of the `Data.p4k` in `<kanalordner>` as
//! files in `<ausgabeordner>`, together with the scmdb build name, so that
//! the slow archive (a network share) is read once and the tests and tools
//! that take `VERSEKIT_DATAFORGE`, `VERSEKIT_GLOBAL_INI` and
//! `VERSEKIT_CONTRACTS` run on the copies. Nothing is written to the game
//! folder, and nothing of it belongs into the repository.

use std::path::Path;

use versekit_gamedata::{ContractFacts, GameData};

/// The file of an extract that holds the p4kx catalog of the contracts.
pub const CONTRACTS_FILE: &str = "contracts.json";

/// The contract facts of the extract in `folder`.
pub fn contract_facts(folder: &Path) -> Result<ContractFacts, String> {
    let bytes = std::fs::read(folder.join(CONTRACTS_FILE))
        .map_err(|e| format!("{CONTRACTS_FILE}: {e} (Auszug mit `versekit-parity auszug` neu ziehen)"))?;
    ContractFacts::from_catalog(&bytes).map_err(|e| format!("{CONTRACTS_FILE}: {e}"))
}

pub fn run(channel: &str, out: &str) -> Result<(), String> {
    let game = GameData::open(channel).map_err(|e| e.to_string())?;
    let out = Path::new(out);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let dcb = game.dataforge_bytes().map_err(|e| e.to_string())?;
    let ini = game.english_ini_bytes().map_err(|e| e.to_string())?;
    let contracts = game.contract_catalog().map_err(|e| e.to_string())?;
    std::fs::write(out.join("Game2.dcb"), &dcb).map_err(|e| e.to_string())?;
    std::fs::write(out.join("global.ini"), &ini).map_err(|e| e.to_string())?;
    std::fs::write(out.join(CONTRACTS_FILE), &contracts).map_err(|e| e.to_string())?;
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
