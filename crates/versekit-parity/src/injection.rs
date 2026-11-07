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

//! Rust side of the parity check of the writing into the game's text file
//! (`tools/parity_injection.py`): one step on a copy, with the inputs read
//! from the data folder as 4.12 reads them.
//!
//! `versekit-parity einspielung <szenario> <global.ini> <sprachordner> <schritt> [<fassung>]`
//!
//! `<szenario>` holds the data folder `daten` (the 4.12 files, flat like
//! `SC_BP_HOME`) and the copy of the game folder; nothing outside it is
//! written. Steps: `einspielen`, `entfernen`, `frisch` (as after placing a
//! new base file), `aufraeumen` (reset a file the note points elsewhere).

use versekit_core::injection::{self, Outcome, sources};
use versekit_core::paths::{Host, Paths};
use versekit_core::writeguard;

pub fn run(scenario: &str, ini: &str, language: &str, step: &str, version: &str) -> Result<(), String> {
    let data = format!("{scenario}/daten");
    let mut host = Host::current();
    host.registry = false;
    host.set_var("SC_BP_HOME", &data);
    host.set_var("SC_BP_NO_NET", "1");
    let paths = Paths::new(host);
    writeguard::deny(true);
    writeguard::allow_under(Some(scenario));
    let outcome = match step {
        "einspielen" => {
            let catalog = paths.app_file(sources::CATALOG_412);
            injection::apply_texts(&paths, ini, language, &sources::inputs(&paths, &catalog, version), false)
        }
        "einspielen-v5" => {
            // The contracts and the reputation built by v5 from scmdb's
            // contract file (`VERSEKIT_MERGED`), the rest as stored.
            let catalog = paths.app_file(sources::CATALOG_412);
            let mut inputs = sources::inputs(&paths, &catalog, version);
            let merged_path = std::env::var("VERSEKIT_MERGED").map_err(|_| "VERSEKIT_MERGED fehlt".to_string())?;
            let merged: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&merged_path).map_err(|e| format!("{merged_path}: {e}"))?)
                    .map_err(|e| format!("{merged_path}: {e}"))?;
            let issues_path = paths.app_file(sources::ISSUES_FILE);
            let issues: serde_json::Value = std::fs::read(&issues_path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or(serde_json::Value::Null);
            let build = issues
                .get("build")
                .and_then(serde_json::Value::as_str)
                .or_else(|| merged.get("version").and_then(serde_json::Value::as_str))
                .unwrap_or_default()
                .to_string();
            let list = versekit_gamedata::corrections::from_scmdb(
                issues.get("daten").unwrap_or(&serde_json::Value::Null),
                &build,
            );
            inputs.missions = versekit_gamedata::contract_info::missions(&merged, &build, &list);
            inputs.reputation = versekit_gamedata::contract_info::reputation(&merged, &build, &list, "");
            injection::apply_texts(&paths, ini, language, &inputs, false)
        }
        "einspielen-katalog" => {
            // As the program reads it: the catalog cache of v5 and the
            // reputation table the catalog build stored.
            let catalog = paths.app_file(sources::CATALOG_V5);
            injection::apply_texts(&paths, ini, language, &sources::inputs(&paths, &catalog, version), false)
        }
        "entfernen" => injection::remove_texts(&paths, ini, language),
        "frisch" => {
            injection::discard_origtext(&paths);
            Outcome::Done(0)
        }
        "aufraeumen" => match injection::clean_leftover(&paths, Some(ini)) {
            Some((_, n)) => Outcome::Done(n),
            None => Outcome::Done(0),
        },
        other => return Err(format!("unbekannter Schritt: {other}")),
    };
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);
    if !blocked.is_empty() {
        return Err(format!("Schreibversuch außerhalb des Szenarios: {blocked:?}"));
    }
    println!("{outcome:?}");
    Ok(())
}
