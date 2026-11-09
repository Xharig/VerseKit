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

//! Rust side of the parity checks.
//!
//! `versekit-parity logscan <python.json> <ausgabe.json> [--gegenprobe]` reads the same files
//! as the Python side (list, `phrasen.json` and language folder come from its
//! output) and writes the result in the same format. It also covers the case
//! without a custom data folder (`phrasen.json` in the `Einstellungen`
//! subfolder), whose paths Rust determines itself.
//!
//! `versekit-parity paths <python.json> <ausgabe.json> [--gegenprobe]`
//! recomputes the data folder, settings and game folder and rewrites every JSON
//! file from the Python side's scratch folder (see `tools/parity_paths.py`).
//!
//! `versekit-parity logsource <python.json> <ausgabe.json> [--gegenprobe]`
//! replays the scenarios for backlog reading, read position, accounts and
//! live tailing (see `tools/parity_logsource.py`).
//!
//! `versekit-parity collection <python.json> <ausgabe.json> [--gegenprobe]`
//! recomputes name mapping and the collection (see `tools/parity_collection.py`).
//!
//! `versekit-parity contracts <python.json> <ausgabe.json> [--gegenprobe]`
//! replays the backlog reading of the contracts log with the playtime and the
//! statistics it starts, on the real logs and on made-up ones (see
//! `tools/parity_contracts.py`).
//!
//! `versekit-parity language <python.json> <ausgabe.json> [--gegenprobe]`
//! renders the user interface texts and language helpers for every case of
//! the Python side (see `tools/parity_language.py`).
//!
//! `versekit-parity theme <python.json> <ausgabe.json> [--gegenprobe]`
//! writes the color schemes under the Python key names (see
//! `tools/parity_theme.py`).
//!
//! `versekit-parity errors <python.json> <ausgabe.json> [--gegenprobe]`
//! redacts the same texts and replays the error log scenario (see
//! `tools/parity_errors.py`).
//!
//! `versekit-parity report <python.json> <ausgabe.json> [--gegenprobe]`
//! builds the error reports from the recorded facts and the same files,
//! gathers the lines from the same data and runs the request pieces; sends
//! only to a made-up server on this computer (see `tools/parity_report.py`).
//!
//! `versekit-parity changelog <python.json> <ausgabe.json> [--gegenprobe]`
//! reads the built-in change logs in both languages with the releases the Python side gives (see
//! `tools/parity_changelog.py`).
//!
//! `versekit-parity thanks <python.json> <ausgabe.json> [--gegenprobe]`
//! lists the sources, people and text keys of the thanks page (see
//! `tools/parity_thanks.py`).
//!
//! `versekit-parity crafting <python.json> <ausgabe.json> [--gegenprobe]`
//! computes what quality makes of every crafted item from the same scmdb
//! files and corrects the same CIG data errors (see
//! `tools/parity_crafting.py`).
//!
//! `versekit-parity craftbook <python.json> <ausgabe.json> [--gegenprobe]`
//! lists everything craftable, sorts it into categories, computes what the
//! recipes ask of a made-up stock and the table of product values (see
//! `tools/parity_craftbook.py`).
//!
//! `versekit-parity demand <python.json> <ausgabe.json> [--gegenprobe]`
//! brings made-up answers of the demand of the unit into their fixed form
//! and tells which materials fit names and when the copy is too old (see
//! `tools/parity_demand.py`).
//!
//! `versekit-parity abbau <python.json> <ausgabe.json> [--gegenprobe]`
//! computes the places and ores of the mining data, the scan signature both
//! ways, the refinery table and the processing methods (see
//! `tools/parity_abbau.py`).
//!
//! `versekit-parity signatur <python.json> <ausgabe.json> [--gegenprobe]`
//! reads the crops of the test sets of the signature scanner, searches the
//! pill in made-up frames and replays teaching (see `tools/parity_signatur.py`).
//!
//! `versekit-parity scmdb <python.json> <ausgabe.json> [--gegenprobe]`
//! reads made-up sections of the log as the bridge to scmdb.net does and runs
//! a session over its server on 127.0.0.1 (see `tools/parity_scmdb.py`).
//!
//! `versekit-parity raffinerie <python.json> <ausgabe.json> [--gegenprobe]`
//! reads made-up words, crops and pictures of the refinery terminal, learns
//! the digits and keeps the jobs (see `tools/parity_raffinerie.py`).
//!
//! `versekit-parity raffinerie-windows <ordner> [--gegenprobe]` enlarges the
//! pictures drawn by the Python side with GDI+, reads them with the text
//! recognition of Windows and whole (see `tools/parity_raffinerie_windows.py`).
//!
//! `versekit-parity felsen <dataforge.dcb> <global.ini> <catalog.json> <mining-data.json> [--gegenprobe]`
//! compares the mineable rocks with their scan signature, the limits of the
//! crafting stations and the cooldowns of the contracts read from the
//! DataForge with the Basetool catalog of p4kx, and tells what the signatures
//! of the game data change in the mining data (see `felsen.rs`).
//!
//! `versekit-parity signatur-zeit [runs]` times one beat of the signature
//! scanner, reading the green grey image alone against reading both.
//!
//! `versekit-parity farm <python.json> <ausgabe.json> [--gegenprobe]`
//! computes the material of the farm notes against a made-up stock, the
//! mining data and the places of what is missing (see `tools/parity_farm.py`).
//!
//! `versekit-parity erkul <python.json> <ausgabe.json> [--gegenprobe]`
//! reads the slots of the ships and the parts of erkul from the two stored
//! files and answers which slots a ship has, which parts fit and what is
//! fitted at the factory (see `tools/parity_erkul.py`).
//!
//! `versekit-parity shipsync <python.json> <ausgabe.json> [--gegenprobe]`
//! computes the identifier of a hangar entry, its insurance, the plan of the
//! sync with the Basetool and the change sets from made-up answers of the
//! Basetool; no request is made (see `tools/parity_shipsync.py`).
//!
//! `versekit-parity exchange <python.json> <ausgabe.json> [--gegenprobe]`
//! computes the plan and the change sets of the blueprint sync and of the
//! stock sync with the Basetool from made-up answers; no request is made (see
//! `tools/parity_exchange.py`).
//!
//! `versekit-parity cart <python.json> <ausgabe.json> [--gegenprobe]`
//! computes the two ways of a part, the bill, the total, the shopping route
//! and the edits of the equipment on a made-up world (see `tools/parity_cart.py`).
//!
//! `versekit-parity werte <dataforge.dcb> <global.ini> <crafting-blueprints.json> [--gegenprobe]`
//! compares the own game values from a DataForge with scmdb's at the same
//! build (see `values_check`).
//!
//! `versekit-parity waren <dataforge.dcb> <global.ini> <ausgabe.json> [--gegenprobe]`
//! lists the goods with a quality and writes `waren-guete.json` beside the
//! output (see `tools/parity_quality_goods.py`).
//!
//! `versekit-parity patch_changes <python.json> <ausgabe.json> [--gegenprobe]`
//! writes the changed game values of a made-up world as rows (see
//! `tools/parity_patch_changes.py`).
//!
//! `versekit-parity overrides <python.json> <ausgabe.json> [--gegenprobe]`
//! asks the own blueprint corrections the names of each scenario (see
//! `tools/parity_overrides.py`).
//!
//! `versekit-parity abbau_katalog <catalog.json> <mining-data.json> [--gegenprobe]`
//! compares the numbers of the mining data with the mining kinds of the catalog p4kx writes from the
//! same build (see `mining_catalog`).
//!
//! `versekit-parity hangar <python.json> <ausgabe.json> [--gegenprobe]`
//! reads the same ship lists into the same start hangars (see
//! `tools/parity_hangar.py`).
//!
//! `versekit-parity einlieferung <hangar.json> <port> <token> [--aus]` runs
//! the hangar receiver on its own, for the same requests as the Python test
//! receiver of the browser extension.
//!
//! `versekit-parity importer <python.json> <ausgabe.json> [--gegenprobe]`
//! reads in the made-up stand files of every format (see
//! `tools/parity_importer.py`).
//!
//! `versekit-parity export <python.json> <ausgabe.json> [--gegenprobe]`
//! saves the collection of every case and writes the output files (see
//! `tools/parity_export.py`).
//!
//! `versekit-parity mission_log <python.json> <ausgabe.json> [--gegenprobe]`
//! loads, merges and searches the mission log and resolves every title (see
//! `tools/parity_mission_log.py`).
//!
//! `versekit-parity stock <python.json> <ausgabe.json> [--gegenprobe]`
//! rebuilds the closed lists, number input and refinery lines of the stock
//! page and replays its file scenarios (see `tools/parity_stock.py`).
//!
//! `versekit-parity stock_backup <python.json> <ausgabe.json> [--gegenprobe]`
//! backs up and reads back the stock files (see `tools/parity_stock_backup.py`).
//!
//! `versekit-parity basetool <python.json> <ausgabe.json> [--gegenprobe]`
//! recomputes the pure functions of the Basetool connection, reads and
//! writes secrets, and replays the protocol scenario against the same
//! made-up Basetool (see `tools/parity_basetool.py`).
//!
//! `versekit-parity orgstock <python.json> <ausgabe.json> [--gegenprobe]`
//! replays the unit storage: fetches, lists, operations, results, lookups
//! and the page's helpers (see `tools/parity_orgstock.py`).
//!
//! `versekit-parity trade <python.json> <ausgabe.json> [--gegenprobe]`
//! replays the queries on the stored UEX lists and the updates on made-up
//! answers of trade (see `tools/parity_trade.py`).
//!
//! `versekit-parity live <python.json> <ausgabe.json> [--gegenprobe]`
//! replays the running contracts of the overlay on a made-up game log (see
//! `tools/parity_live.py`).
//!
//! `versekit-parity bauplan_quelle <daten> <kanalordner> [--gegenprobe [haken|stern|raute]]`
//! compares the blueprint list's catalog from the 4.12 cache with the one
//! from the `Data.p4k` and measures both loads (see `blueprint_source`).
//!
//! `versekit-parity grafik <python.json> <ausgabe.json> [--gegenprobe]`
//! runs the graphics settings of `attributes.xml` (the text after a set of
//! changes, the settings and levels read, the comparison with the preset of
//! the machine) as `tools/parity_grafik.py` does with `graphics.py` (see
//! `grafik`).
//!
//! `versekit-parity schreiben <python.json> <ausgabe.json> [--gegenprobe]`
//! runs the changes of the `actionmaps.xml` (bind an action, reset, play a
//! binding back, swap an id) as `tools/parity_schreiben.py` does with
//! `joysticks.py`, comparing the meaning of the result (see `schreiben`).
//!
//! `versekit-parity achsen <python.json> <ausgabe.json> [--gegenprobe]`
//! runs the reading of the dead zones, saturations and exponents, the answer of
//! the curve and the numbers the way Python writes them, as
//! `tools/parity_achsen.py` does with `curves.py` (see `achsen`).
//!
//! `versekit-parity belegung <python.json> <ausgabe.json> [--gegenprobe]`
//! runs what lies on the keys (plain names of inputs and actions, the
//! `defaultProfile.xml`, the four views, the list as a file) as
//! `tools/parity_belegung.py` does with `joysticks.py` (see `belegung`).
//!
//! `versekit-parity joysticks <python.json> <ausgabe.json> [--gegenprobe]`
//! runs the reading part of the controls (devices of a log, the binding and
//! the bindings in an `actionmaps.xml`, the comparison over a scratch game
//! folder) as `tools/parity_joysticks.py` does with `joysticks.py` (see
//! `joysticks`).
//!
//! `versekit-parity einspielschutz <python.json> <ausgabe.json> [--gegenprobe]`
//! runs the commands of a backup (start programs, start command, own address
//! of the translation) through the protection when a backup is restored, as
//! `tools/parity_einspielschutz.py` does with `backup._safe_settings` (see
//! `einspielschutz`).
//!
//! `versekit-parity sicherung <python.json> <ausgabe.json> [--gegenprobe]`
//! runs the backup (which files come along, the settings made safe, what a file is, a whole restore, a whole
//! backup) through the cases `tools/parity_sicherung.py` wrote with the answers of `backup.py` (see
//! `sicherung`).
//!
//! `versekit-parity phrases_learn <python.json> <ausgabe.json> [--gegenprobe]` works the blueprint phrase out
//! of the same logs and keeps it in the same `phrasen.json` files as `tools/parity_phrases_learn.py` (see
//! `phrases_learn`).
//!
//! `versekit-parity usage <python.json> <ausgabe.json> [--gegenprobe]` builds
//! the daily usage report and runs the page counters with the settings,
//! counters and steps `tools/parity_usage.py` wrote (see `usage`).
//!
//! `versekit-parity lagerlog <python.json> <ausgabe.json> [--gegenprobe]`
//! counts the purchases, the inventory moves and the places of the same game
//! logs as the Python side, which counts them with patterns of its own (see
//! `tools/parity_lagerlog.py`).
//!
//! `versekit-parity katalog_start <daten> <kanalordner> [--gegenprobe]`
//! measures how long the list waits for its catalog on the first start after
//! a patch, with the real `Data.p4k` (see `catalog_start`).
//!
//! `versekit-parity katalog_luecke <daten> <auszug> [<scmdb-ordner>] [--alle]`
//! compares the catalog built from the kept game data with the 4.12 cache
//! (see `catalog_gap`); `versekit-parity katalog_bau <daten> <kanalordner>
//! <scmdb-ordner> [--alle]` goes the whole way of a new user without a 4.12
//! cache with the real `Data.p4k` (see `catalog_build`).
//!
//! `versekit-parity auftragsangaben <merged.json> <katalog-cache.json>
//! <auftragsruf.json> [<cig-datenfehler.json>]` builds the contract details
//! of the blueprint notes from scmdb's file and compares them with the 4.12
//! cache and the reputation table (see `contract_info`).
//!
//! `versekit-parity auszug <kanalordner> <ausgabeordner>` keeps the DataForge
//! and the English `global.ini` of a `Data.p4k` as two files, so that tools
//! and tests read the slow archive once (see `extract`); `katalogteil
//! <kanalordner> <filter> <datei>` keeps the p4kx catalog of the records a
//! filter selects.
//!
//! `versekit-parity rufstufen` and `auftragsruf` compare the reputation read
//! from the game data with the stored tables (see `ruf`).

mod abbau;
mod achsen;
mod asop;
mod basetool;
mod belegung;
mod blueprint_source;
mod cart;
mod catalog_build;
mod catalog_gap;
mod catalog_start;
mod changelog;
mod collection;
mod contract_info;
mod contracts;
mod craftbook;
mod crafting;
mod demand;
mod dump;
mod einspielschutz;
mod erkul;
mod errorlog;
mod exchange;
mod export;
mod extract;
mod importer;
mod farm;
mod felsen;
mod grafik;
mod hangar;
mod hangar_save;
mod injection;
mod joysticks;
mod lagerlog;
mod language;
mod live;
mod logsource;
mod mining_catalog;
mod mission_log;
mod orgstock;
mod overrides;
mod patch_changes;
mod paths;
mod phrases_learn;
mod raffinerie;
mod raffinerie_windows;
mod report;
mod ruf;
mod scmdb;
mod schreiben;
mod shipsync;
mod sicherung;
mod signatur;
mod signatur_zeit;
mod start;
mod stock;
mod stock_backup;
mod thanks;
mod theme;
mod trade;
mod usage;
mod vollstaendig;
mod values_check;
mod waren;

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{Value, json};
use versekit_core::logscan::{Find, account_of_file, read_file};
use versekit_core::logsource::LogSource;
use versekit_core::paths::{Host, Paths};
use versekit_core::phrases::{Origin, Sources, pattern};
use versekit_core::writeguard;

fn path_or_none(value: &Value) -> Option<PathBuf> {
    value.as_str().filter(|s| !s.is_empty()).map(PathBuf::from)
}

fn find_json((name, extra): &Find) -> Value {
    match extra {
        None => json!([name, null]),
        Some((a, b, c)) => json!([name, [a, b, c]]),
    }
}

/// With `counter_check`, drops one find, changes one account and adds a
/// phrase to the case without a data folder, so the comparison must report
/// all three.
fn logscan(python_json: &str, out: &str, counter_check: bool) -> Result<(), String> {
    let input: Value =
        serde_json::from_slice(&std::fs::read(python_json).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let sources = Sources {
        phrases_file: path_or_none(&input["phrases_file"]),
        localization_folder: path_or_none(&input["localization_folder"]),
    };
    let (phrases, origin) = sources.collect();
    let rx = pattern(&phrases);
    let mut files = Vec::new();
    for entry in input["files"].as_array().into_iter().flatten() {
        let Some(name) = entry["file"].as_str() else { continue };
        let path = PathBuf::from(name);
        let finds: Vec<Value> = read_file(&path, &rx, &sources).iter().map(find_json).collect();
        files.push(json!({ "file": name, "account": account_of_file(&path), "finds": finds }));
    }
    if counter_check {
        let with_finds = files
            .iter_mut()
            .find(|f| f["finds"].as_array().is_some_and(|a| !a.is_empty()))
            .ok_or("Gegenprobe: keine Datei mit Funden")?;
        if let Some(finds) = with_finds["finds"].as_array_mut() {
            finds.pop();
        }
        let first = files.first_mut().ok_or("Gegenprobe: keine Datei")?;
        first["account"] = json!("gegenprobe");
    }
    let mut result = json!({ "phrases": phrases, "origin": origin_name(origin), "files": files });
    if input["subfolder"].is_object() {
        let mut case = subfolder_case(&input["subfolder"])?;
        if counter_check {
            case["phrases"]
                .as_array_mut()
                .ok_or("Gegenprobe: Fall ohne Datenordner ohne Formulierungen")?
                .push(json!("gegenprobe"));
        }
        result["subfolder"] = case;
    } else if counter_check {
        return Err("Gegenprobe: Fall ohne Datenordner fehlt in der Python-Ausgabe".into());
    }
    std::fs::write(out, serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn origin_name(origin: Origin) -> &'static str {
    match origin {
        Origin::Own => "eigen",
        Origin::Ini => "ini",
        Origin::Table => "tabelle",
    }
}

/// Without a custom data folder: home and config folders point into the
/// Python side's scratch folder, the registry stays off. The phrases
/// come through the same route as in the program (`LogSource::sources_of`).
fn subfolder_case(case: &Value) -> Result<Value, String> {
    let scratch = case["scratch"].as_str().ok_or("subfolder.scratch fehlt")?;
    let mut host = Host::current();
    host.registry = false;
    for name in case["removed"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        host.remove_var(name);
    }
    for (name, value) in case["set"].as_object().into_iter().flatten() {
        if let Some(value) = value.as_str() {
            host.set_var(name, value);
        }
    }
    writeguard::deny(true);
    writeguard::allow_under(Some(scratch));
    let paths = Paths::new(host);
    let sources = LogSource::sources_of(&paths);
    let (phrases, origin) = sources.collect();
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);
    let text = |p: &Option<PathBuf>| p.as_ref().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(json!({
        "phrases_file": text(&sources.phrases_file),
        "localization_folder": text(&sources.localization_folder),
        "phrases": phrases,
        "origin": origin_name(origin),
        "blocked": blocked,
    }))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [cmd, input, out] if cmd == "logscan" => logscan(input, out, false),
        [cmd, input, out, flag] if cmd == "logscan" && flag == "--gegenprobe" => logscan(input, out, true),
        [cmd, input, out] if cmd == "phrases_learn" => phrases_learn::run(input, out, false),
        [cmd, input, out, flag] if cmd == "phrases_learn" && flag == "--gegenprobe" => {
            phrases_learn::run(input, out, true)
        }
        [cmd, input, out] if cmd == "paths" => paths::run(input, out, false),
        [cmd, input, out, flag] if cmd == "paths" && flag == "--gegenprobe" => paths::run(input, out, true),
        [cmd, input, out] if cmd == "logsource" => logsource::run(input, out, false),
        [cmd, input, out, flag] if cmd == "logsource" && flag == "--gegenprobe" => logsource::run(input, out, true),
        [cmd, input, out] if cmd == "contracts" => contracts::run(input, out, false),
        [cmd, input, out, flag] if cmd == "contracts" && flag == "--gegenprobe" => contracts::run(input, out, true),
        [cmd, input, out] if cmd == "collection" => collection::run(input, out, false),
        [cmd, input, out, flag] if cmd == "collection" && flag == "--gegenprobe" => collection::run(input, out, true),
        [cmd, input, out] if cmd == "language" => language::run(input, out, false),
        [cmd, input, out, flag] if cmd == "language" && flag == "--gegenprobe" => language::run(input, out, true),
        [cmd, input, out] if cmd == "theme" => theme::run(input, out, false),
        [cmd, input, out, flag] if cmd == "theme" && flag == "--gegenprobe" => theme::run(input, out, true),
        [cmd, input, out] if cmd == "hangar" => hangar::run(input, out, false),
        [cmd, input, out, flag] if cmd == "hangar" && flag == "--gegenprobe" => hangar::run(input, out, true),
        [cmd, input, out] if cmd == "hangar_save" => hangar_save::run(input, out, hangar_save::Counter::default()),
        [cmd, input, out, flag, which] if cmd == "hangar_save" && flag == "--gegenprobe" => {
            hangar_save::Counter::parse(Some(which)).and_then(|counter| hangar_save::run(input, out, counter))
        }
        [cmd, file, port, token] if cmd == "einlieferung" => hangar::serve(file, port, token, false),
        [cmd, file, port, token, flag] if cmd == "einlieferung" && flag == "--aus" => {
            hangar::serve(file, port, token, true)
        }
        [cmd, input, out] if cmd == "errors" => errorlog::run(input, out, false),
        [cmd, input, out, flag] if cmd == "errors" && flag == "--gegenprobe" => errorlog::run(input, out, true),
        [cmd, input, out] if cmd == "report" => report::run(input, out, false),
        [cmd, input, out, flag] if cmd == "report" && flag == "--gegenprobe" => report::run(input, out, true),
        [cmd, input, out] if cmd == "demand" => demand::run(input, out, false),
        [cmd, input, out, flag] if cmd == "demand" && flag == "--gegenprobe" => demand::run(input, out, true),
        [cmd, input, out] if cmd == "grafik" => grafik::run(input, out, false),
        [cmd, input, out, flag] if cmd == "grafik" && flag == "--gegenprobe" => grafik::run(input, out, true),
        [cmd, input, out] if cmd == "achsen" => achsen::run(input, out, false),
        [cmd, input, out, flag] if cmd == "achsen" && flag == "--gegenprobe" => achsen::run(input, out, true),
        [cmd, input, out] if cmd == "schreiben" => schreiben::run(input, out, false),
        [cmd, input, out, flag] if cmd == "schreiben" && flag == "--gegenprobe" => schreiben::run(input, out, true),
        [cmd, input, out] if cmd == "belegung" => belegung::run(input, out, false),
        [cmd, input, out, flag] if cmd == "belegung" && flag == "--gegenprobe" => belegung::run(input, out, true),
        [cmd, input, out] if cmd == "joysticks" => joysticks::run(input, out, false),
        [cmd, input, out, flag] if cmd == "joysticks" && flag == "--gegenprobe" => joysticks::run(input, out, true),
        [cmd, input, out] if cmd == "einspielschutz" => einspielschutz::run(input, out, false),
        [cmd, input, out, flag] if cmd == "einspielschutz" && flag == "--gegenprobe" => {
            einspielschutz::run(input, out, true)
        }
        [cmd, input, out] if cmd == "sicherung" => sicherung::run(input, out, false),
        [cmd, input, out, flag] if cmd == "sicherung" && flag == "--gegenprobe" => sicherung::run(input, out, true),
        [cmd, input, out] if cmd == "start" => start::run(input, out, false),
        [cmd, input, out, flag] if cmd == "start" && flag == "--gegenprobe" => start::run(input, out, true),
        [cmd, input, out] if cmd == "erkul" => erkul::run(input, out, false),
        [cmd, input, out, flag] if cmd == "erkul" && flag == "--gegenprobe" => erkul::run(input, out, true),
        [cmd, input, out] if cmd == "shipsync" => shipsync::run(input, out, false),
        [cmd, input, out, flag] if cmd == "shipsync" && flag == "--gegenprobe" => shipsync::run(input, out, true),
        [cmd, input, out] if cmd == "exchange" => exchange::run(input, out, false),
        [cmd, input, out, flag] if cmd == "exchange" && flag == "--gegenprobe" => exchange::run(input, out, true),
        [cmd, input, out] if cmd == "cart" => cart::run(input, out, false),
        [cmd, input, out, flag] if cmd == "cart" && flag == "--gegenprobe" => cart::run(input, out, true),
        [cmd, dcb, ini, catalog, mining] if cmd == "felsen" => felsen::run(dcb, ini, catalog, mining, false),
        [cmd, dcb, ini, catalog, mining, flag] if cmd == "felsen" && flag == "--gegenprobe" => {
            felsen::run(dcb, ini, catalog, mining, true)
        }
        [cmd] if cmd == "signatur-zeit" => signatur_zeit::run(15),
        [cmd, runs] if cmd == "signatur-zeit" => {
            runs.parse().map_err(|_| "Anzahl der Laeufe".to_string()).and_then(signatur_zeit::run)
        }
        [cmd, folder] if cmd == "raffinerie-windows" => raffinerie_windows::run(folder, false),
        [cmd, folder, flag] if cmd == "raffinerie-windows" && flag == "--gegenprobe" => {
            raffinerie_windows::run(folder, true)
        }
        [cmd, input, out] if cmd == "scmdb" => scmdb::run(input, out, false),
        [cmd, input, out, flag] if cmd == "scmdb" && flag == "--gegenprobe" => scmdb::run(input, out, true),
        [cmd, input, out] if cmd == "raffinerie" => raffinerie::run(input, out, false),
        [cmd, input, out, flag] if cmd == "raffinerie" && flag == "--gegenprobe" => raffinerie::run(input, out, true),
        [cmd, input, out] if cmd == "signatur" => signatur::run(input, out, false),
        [cmd, input, out, flag] if cmd == "signatur" && flag == "--gegenprobe" => signatur::run(input, out, true),
        [cmd, input, out] if cmd == "abbau" => abbau::run(input, out, false),
        [cmd, input, out, flag] if cmd == "abbau" && flag == "--gegenprobe" => abbau::run(input, out, true),
        [cmd, input, out] if cmd == "farm" => farm::run(input, out, false),
        [cmd, input, out, flag] if cmd == "farm" && flag == "--gegenprobe" => farm::run(input, out, true),
        [cmd, input, out] if cmd == "craftbook" => craftbook::run(input, out, false),
        [cmd, input, out, flag] if cmd == "craftbook" && flag == "--gegenprobe" => craftbook::run(input, out, true),
        [cmd, input, out] if cmd == "crafting" => crafting::run(input, out, false),
        [cmd, input, out, flag] if cmd == "crafting" && flag == "--gegenprobe" => crafting::run(input, out, true),
        [cmd, catalog, mining] if cmd == "abbau_katalog" => mining_catalog::run(catalog, mining, false),
        [cmd, catalog, mining, flag] if cmd == "abbau_katalog" && flag == "--gegenprobe" => {
            mining_catalog::run(catalog, mining, true)
        }
        [cmd, dcb, ini, recipes] if cmd == "werte" => values_check::run(dcb, ini, recipes, false),
        [cmd, dcb, ini, recipes, flag] if cmd == "werte" && flag == "--gegenprobe" => {
            values_check::run(dcb, ini, recipes, true)
        }
        [cmd, input, out] if cmd == "overrides" => overrides::run(input, out, false),
        [cmd, input, out, flag] if cmd == "overrides" && flag == "--gegenprobe" => overrides::run(input, out, true),
        [cmd, input, out] if cmd == "patch_changes" => patch_changes::run(input, out, false),
        [cmd, input, out, flag] if cmd == "patch_changes" && flag == "--gegenprobe" => {
            patch_changes::run(input, out, true)
        }
        [cmd, dcb, ini, out] if cmd == "waren" => waren::run(dcb, ini, out, false),
        [cmd, dcb, ini, out, flag] if cmd == "waren" && flag == "--gegenprobe" => waren::run(dcb, ini, out, true),
        [cmd, input, out] if cmd == "changelog" => changelog::run(input, out, false),
        [cmd, input, out, flag] if cmd == "changelog" && flag == "--gegenprobe" => changelog::run(input, out, true),
        [cmd, input, out] if cmd == "thanks" => thanks::run(input, out, false),
        [cmd, input, out, flag] if cmd == "thanks" && flag == "--gegenprobe" => thanks::run(input, out, true),
        [cmd, input, out] if cmd == "importer" => importer::run(input, out, false),
        [cmd, input, out, flag] if cmd == "importer" && flag == "--gegenprobe" => importer::run(input, out, true),
        [cmd, input, out] if cmd == "export" => export::run(input, out, false),
        [cmd, input, out, flag] if cmd == "export" && flag == "--gegenprobe" => export::run(input, out, true),
        [cmd, input, out] if cmd == "mission_log" => mission_log::run(input, out, false),
        [cmd, input, out, flag] if cmd == "mission_log" && flag == "--gegenprobe" => mission_log::run(input, out, true),
        [cmd, input, out] if cmd == "stock" => stock::run(input, out, false),
        [cmd, input, out, flag] if cmd == "stock" && flag == "--gegenprobe" => stock::run(input, out, true),
        [cmd, input, out] if cmd == "asop" => asop::run(input, out, false),
        [cmd, input, out, flag] if cmd == "asop" && flag == "--gegenprobe" => asop::run(input, out, true),
        [cmd, input, out] if cmd == "stock_backup" => stock_backup::run(input, out, false),
        [cmd, input, out, flag] if cmd == "stock_backup" && flag == "--gegenprobe" => {
            stock_backup::run(input, out, true)
        }
        [cmd, input, out] if cmd == "trade" => trade::run(input, out, false),
        [cmd, input, out, flag] if cmd == "trade" && flag == "--gegenprobe" => trade::run(input, out, true),
        [cmd, input, out] if cmd == "live" => live::run(input, out, false),
        [cmd, input, out, flag] if cmd == "live" && flag == "--gegenprobe" => live::run(input, out, true),
        [cmd, input, out] if cmd == "orgstock" => orgstock::run(input, out, false),
        [cmd, input, out, flag] if cmd == "orgstock" && flag == "--gegenprobe" => orgstock::run(input, out, true),
        [cmd, input, out] if cmd == "basetool" => basetool::run(input, out, false),
        [cmd, input, out, flag] if cmd == "basetool" && flag == "--gegenprobe" => basetool::run(input, out, true),
        [cmd, data, channel] if cmd == "bauplan_quelle" => {
            blueprint_source::run(data, channel, blueprint_source::Counter::Off)
        }
        [cmd, data, channel, flag, which @ ..]
            if cmd == "bauplan_quelle" && flag == "--gegenprobe" && which.len() <= 1 =>
        {
            blueprint_source::Counter::parse(which.first().map(String::as_str))
                .and_then(|counter| blueprint_source::run(data, channel, counter))
        }
        [cmd, data, extract, rest @ ..] if cmd == "katalog_luecke" && rest.len() <= 2 => {
            let all = rest.iter().any(|arg| arg == "--alle");
            let scmdb = rest.iter().find(|arg| *arg != "--alle").map(String::as_str);
            catalog_gap::run(data, extract, scmdb, all)
        }
        [cmd, extract, kind, part, pattern @ ..] if cmd == "satz" && pattern.len() <= 1 => {
            dump::run(extract, kind, part, pattern.first().map(String::as_str))
        }
        [cmd, data, channel, scmdb, rest @ ..] if cmd == "katalog_bau" && rest.len() <= 2 => {
            let flag = |name: &str| rest.iter().any(|arg| arg == name);
            catalog_build::run(data, channel, scmdb, flag("--alle"), flag("--aufruester"))
        }
        [cmd, channel, out] if cmd == "auszug" => extract::run(channel, out),
        [cmd, channel, filter, out] if cmd == "katalogteil" => extract::part(channel, filter, out),
        [cmd, catalog, stored] if cmd == "rufstufen" => ruf::ranks(catalog, stored),
        [cmd, folder, merged, stored] if cmd == "auftragsruf" => ruf::contracts(folder, merged, stored),
        [cmd, folder, merged] if cmd == "auftraege_vollstaendig" => vollstaendig::run(folder, merged, None),
        [cmd, folder, merged, flag, title] if cmd == "auftraege_vollstaendig" && flag == "--weglassen" => {
            vollstaendig::run(folder, merged, Some(title))
        }
        [cmd, merged, cache, reputation, rest @ ..] if cmd == "auftragsangaben" && rest.len() <= 1 => {
            contract_info::run(merged, cache, reputation, rest.first().map(String::as_str))
        }
        [cmd, scenario, ini, language, step] if cmd == "einspielung" => {
            injection::run(scenario, ini, language, step, "4.12.0")
        }
        [cmd, scenario, ini, language, step, version] if cmd == "einspielung" => {
            injection::run(scenario, ini, language, step, version)
        }
        [cmd, data, channel] if cmd == "katalog_start" => catalog_start::run(data, channel, false),
        [cmd, data, channel, flag] if cmd == "katalog_start" && flag == "--gegenprobe" => {
            catalog_start::run(data, channel, true)
        }
        [cmd, input, out] if cmd == "lagerlog" => lagerlog::run(input, out, false),
        [cmd, input, out, flag] if cmd == "lagerlog" && flag == "--gegenprobe" => lagerlog::run(input, out, true),
        [cmd, input, out] if cmd == "usage" => usage::run(input, out, false),
        [cmd, input, out, flag] if cmd == "usage" && flag == "--gegenprobe" => usage::run(input, out, true),
        _ => Err("Aufruf: versekit-parity logscan <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity paths <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity phrases_learn <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity logsource <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity collection <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity contracts <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity language <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity theme <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity errors <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity report <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity thanks <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity changelog <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity abbau_katalog <catalog.json> <mining-data.json> [--gegenprobe]\n       \
                  versekit-parity waren <dataforge.dcb> <global.ini> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity overrides <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity patch_changes <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity hangar <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity hangar_save <python.json> <ausgabe.json> [--gegenprobe format|merge|schiffe]\n       \
                  versekit-parity einlieferung <hangar.json> <port> <token> [--aus]\n       \
                  versekit-parity export <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity importer <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity mission_log <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity stock <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity asop <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity stock_backup <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity basetool <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity orgstock <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity trade <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity live <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity bauplan_quelle <daten> <kanalordner> [--gegenprobe [haken|stern|raute]]\n       \
                  versekit-parity einspielschutz <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity sicherung <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity grafik <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity joysticks <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity achsen <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity belegung <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity schreiben <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity katalog_start <daten> <kanalordner> [--gegenprobe]\n       \
                  versekit-parity usage <python.json> <ausgabe.json> [--gegenprobe]\n       \
                  versekit-parity auszug <kanalordner|p4k-kopie> <ausgabeordner>\n       \
                  versekit-parity katalogteil <kanalordner|p4k-kopie> <filter> <datei>\n       \
                  versekit-parity rufstufen <reputation.json> <rufstufen.json>\n       \
                  versekit-parity auftragsruf <auszug> <merged.json> <auftragsruf.json>\n       \
                  versekit-parity auftraege_vollstaendig <auszug> <merged.json> [--weglassen <titel>]\n       \
                  versekit-parity auftragsangaben <merged.json> <katalog-cache.json> <auftragsruf.json> [<cig-datenfehler.json>]\n       \
                  versekit-parity einspielung <szenario> <global.ini> <sprachordner> <schritt> [<fassung>]\n       \
                  versekit-parity katalog_luecke <daten> <auszug> [<scmdb-ordner>] [--alle]\n       \
                  versekit-parity katalog_bau <daten> <kanalordner> <scmdb-ordner> [--alle] [--aufruester]\n       \
                  versekit-parity satz <auszug> <typ> <pfadteil> [<muster>]"
            .into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
