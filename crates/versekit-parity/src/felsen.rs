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

//! Cross-check of what VerseKit reads itself from the DataForge against the Basetool catalog of p4kx (0.4.1 and
//! later) written from the same build: the mineable rocks with their scan signature (`mineableEntities`), the
//! crafting stations with the limits of their queues (`crafters`) and the cooldowns of the contracts. On top of
//! that what the signatures of the game data do to the mining data: which signatures of the file they replace and
//! which values the scanner can read besides.
//!
//! `versekit-parity felsen <dataforge.dcb> <global.ini> <catalog.json> <mining-data.json> [--gegenprobe]`. With
//! `--gegenprobe` one signature of a rock, one limit of a queue, one cooldown and one signature of the mining
//! data are changed, and the check must report each of the four.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use p4kx_dataforge::{DataForge, StructValue, TreeBuilder, Value as DfValue};
use serde_json::Value;
use versekit_core::mining_data::{self, MiningData};
use versekit_core::signature_scan;
use versekit_gamedata::mineables::{self, Crafter, Rock};
use versekit_gamedata::{ContractFacts, Language, Localization, blueprints_from_dataforge};

fn read(file: &str) -> Result<Value, String> {
    serde_json::from_slice(&std::fs::read(file).map_err(|e| format!("{file}: {e}"))?)
        .map_err(|e| format!("{file}: {e}"))
}

fn items<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn by_guid(list: &[Value]) -> BTreeMap<&str, &Value> {
    list.iter().filter_map(|entry| Some((entry.get("guid")?.as_str()?, entry))).collect()
}

fn float32(value: &Value) -> Option<f32> {
    value.as_f64().map(|x| x as f32)
}

/// How many of a kind agree and how many do not, with the first few cases.
#[derive(Debug, Default)]
struct Count {
    equal: usize,
    different: usize,
    examples: Vec<String>,
}

impl Count {
    fn add(&mut self, same: bool, example: impl FnOnce() -> String) {
        if same {
            self.equal += 1;
        } else {
            self.different += 1;
            if self.examples.len() < 4 {
                self.examples.push(example());
            }
        }
    }
}

#[derive(Debug, Default)]
struct Report {
    rocks: Count,
    deposits: Count,
    rock_only_ours: Vec<String>,
    rock_only_catalog: Vec<String>,
    crafters: Count,
    crafter_only_ours: Vec<String>,
    crafter_only_catalog: Vec<String>,
    cooldowns: Count,
    cooldown_without_contract: usize,
    /// Sources whose cooldown differs from the catalog because the contract states its own.
    cooldown_own: usize,
    abandoned: usize,
    elements: Count,
    replaced: Vec<String>,
    asteroids: Vec<(String, f64)>,
    new_values: Vec<u64>,
    lost_values: Vec<u64>,
}

impl Report {
    fn differences(&self) -> usize {
        self.rocks.different
            + self.deposits.different
            + self.rock_only_ours.len()
            + self.rock_only_catalog.len()
            + self.crafters.different
            + self.crafter_only_ours.len()
            + self.crafter_only_catalog.len()
            + self.cooldowns.different
            + self.elements.different
    }

    fn text(&self) -> String {
        let mut out = String::new();
        let mut line = |name: &str, count: &Count| {
            let _ = writeln!(out, "{name}: {} gleich, {} verschieden", count.equal, count.different);
            for example in &count.examples {
                let _ = writeln!(out, "    {example}");
            }
        };
        line("Steine (Klasse, Pfad, Signaturen, Scan-Signatur)", &self.rocks);
        line("Name der Ablagerung", &self.deposits);
        line("Werkbänke (Grenzen der Warteschlangen)", &self.crafters);
        line("Sperrzeit der Aufträge", &self.cooldowns);
        line("Scan-Signatur der Rohstoffe gegen mining-data.json", &self.elements);
        let _ = writeln!(out, "Stein nur bei uns: {:?}", self.rock_only_ours);
        let _ = writeln!(out, "Stein nur im Katalog: {:?}", self.rock_only_catalog);
        let _ = writeln!(out, "Werkbank nur bei uns: {:?}", self.crafter_only_ours);
        let _ = writeln!(out, "Werkbank nur im Katalog: {:?}", self.crafter_only_catalog);
        let _ = writeln!(
            out,
            "Sperrzeit anders als im Katalog, weil der Auftrag eine eigene nennt (der Katalog liest nur die Gruppe): {}",
            self.cooldown_own
        );
        let _ = writeln!(out, "Quellen ohne Auftrag im Katalog: {}", self.cooldown_without_contract);
        let _ = writeln!(out, "Aufträge mit Abbruch-Sperre (abandonedCooldownTime) im Katalog: {}", self.abandoned);
        let _ =
            writeln!(out, "Hinweis, die Signaturen der Spieldaten ersetzen in mining-data.json: {:?}", self.replaced);
        let _ = writeln!(out, "Hinweis, Asteroidentypen dazu ({}):", self.asteroids.len());
        for (name, signature) in &self.asteroids {
            let _ = writeln!(out, "    {name}: {signature}");
        }
        let _ = writeln!(
            out,
            "Hinweis, Werte, die der Scanner dazu liest ({}): {:?}",
            self.new_values.len(),
            self.new_values
        );
        let _ =
            writeln!(out, "Hinweis, Werte, die weggefallen sind ({}): {:?}", self.lost_values.len(), self.lost_values);
        let _ = writeln!(out, "Abweichungen gesamt: {}", self.differences());
        out
    }
}

fn compare_rocks(report: &mut Report, ours: &[Rock], catalog: &Value) {
    let theirs = by_guid(items(catalog, "mineableEntities"));
    let compositions = by_guid(items(catalog, "mineableCompositions"));
    let mine: BTreeMap<&str, &Rock> = ours.iter().map(|r| (r.guid.as_str(), r)).collect();
    report.rock_only_ours = mine.keys().filter(|g| !theirs.contains_key(**g)).map(|g| (*g).to_string()).collect();
    report.rock_only_catalog = theirs.keys().filter(|g| !mine.contains_key(**g)).map(|g| (*g).to_string()).collect();
    for (guid, rock) in &mine {
        let Some(entry) = theirs.get(guid) else { continue };
        let signatures: Vec<Option<f32>> = items(entry, "signatures").iter().map(float32).collect();
        let ours_signatures: Vec<Option<f32>> = rock.signatures.iter().map(|s| Some(*s)).collect();
        let scan = float32(&entry["scanSignature"]);
        let same = entry["className"].as_str() == Some(rock.class_name.as_str())
            && entry["recordPath"].as_str() == Some(rock.record_path.as_str())
            && signatures == ours_signatures
            && scan == rock.scan_signature;
        report.rocks.add(same, || {
            format!(
                "{}: Signaturen {:?} gegen {:?}, Scan {:?} gegen {:?}",
                rock.class_name, rock.signatures, signatures, rock.scan_signature, scan
            )
        });
        let deposit = entry["mineableCompositionGuid"]
            .as_str()
            .and_then(|g| compositions.get(g))
            .and_then(|c| c["depositName"]["en"].as_str());
        report.deposits.add(deposit == rock.deposit.as_deref(), || {
            format!("{}: {:?} gegen {:?}", rock.class_name, rock.deposit, deposit)
        });
    }
}

fn compare_crafters(report: &mut Report, ours: &[Crafter], catalog: &Value) {
    let theirs = by_guid(items(catalog, "crafters"));
    let mine: BTreeMap<&str, &Crafter> = ours.iter().map(|c| (c.guid.as_str(), c)).collect();
    report.crafter_only_ours = mine.keys().filter(|g| !theirs.contains_key(**g)).map(|g| (*g).to_string()).collect();
    report.crafter_only_catalog = theirs.keys().filter(|g| !mine.contains_key(**g)).map(|g| (*g).to_string()).collect();
    for (guid, crafter) in &mine {
        let Some(entry) = theirs.get(guid) else { continue };
        let queues: Vec<(Option<String>, Option<i64>, Option<i64>)> = items(entry, "queues")
            .iter()
            .map(|q| {
                (q["name"].as_str().map(str::to_string), q["maxJobsInProgress"].as_i64(), q["maxJobsWaiting"].as_i64())
            })
            .collect();
        let ours_queues: Vec<_> =
            crafter.queues.iter().map(|q| (q.name.clone(), q.max_jobs_in_progress, q.max_jobs_waiting)).collect();
        let same = entry["className"].as_str() == Some(crafter.class_name.as_str())
            && entry["recordPath"].as_str() == Some(crafter.record_path.as_str())
            && entry["maxJobsInHistory"].as_i64() == crafter.max_jobs_in_history
            && queues == ours_queues;
        report.crafters.add(same, || {
            format!(
                "{}: Verlauf {:?} gegen {:?}, Warteschlangen {:?} gegen {:?}",
                crafter.class_name,
                crafter.max_jobs_in_history,
                entry["maxJobsInHistory"].as_i64(),
                ours_queues,
                queues
            )
        });
    }
}

/// The cooldown of a contract as VerseKit keeps it: the personal cooldown where the contract has one.
fn catalog_cooldown(contract: &Value) -> Option<i64> {
    if contract["hasPersonalCooldown"].as_bool() != Some(true) {
        return None;
    }
    contract["personalCooldownTime"].as_f64().map(|t| t as i64)
}

fn array<'t, 'f>(parent: &'t StructValue<'f>, name: &str) -> &'t [DfValue<'f>] {
    match parent.field(name) {
        Some(DfValue::Array(items)) => items,
        _ => &[],
    }
}

fn structs<'t, 'f>(parent: &'t StructValue<'f>, name: &str) -> impl Iterator<Item = &'t StructValue<'f>> {
    array(parent, name).iter().filter_map(|value| if let DfValue::Struct(s) = value { Some(s) } else { None })
}

/// Does the parameter set state the personal cooldown of its own?
fn states_cooldown(layer: Option<&StructValue<'_>>) -> bool {
    let Some(layer) = layer else { return false };
    ["boolParamOverrides", "intParamOverrides"].into_iter().any(|kind| {
        structs(layer, kind).any(|param| {
            matches!(
                param.field("param"),
                Some(DfValue::String(name) | DfValue::Enum(name))
                    if *name == "HasPersonalCooldown" || *name == "PersonalCooldownTime"
            )
        })
    })
}

/// The contracts (generator path and debug name) whose own parameter sets, or those of a sub contract, state the
/// personal cooldown. The catalog reads the cooldown of the handler only; VerseKit reads these first.
fn contracts_with_own_cooldown(dcb: &[u8]) -> Result<BTreeSet<(String, String)>, String> {
    let forge = DataForge::open(dcb).map_err(|e| e.to_string())?;
    let mut builder = TreeBuilder::new();
    let mut found = BTreeSet::new();
    for record in forge.records() {
        if record.type_name().ok() != Some("ContractGenerator") {
            continue;
        }
        let Ok(tree) = builder.build(&record) else { continue };
        for handler in structs(tree.root(), "generators") {
            for contract in structs(handler, "contracts") {
                let own = |layer: &StructValue<'_>| match layer.field("paramOverrides") {
                    Some(DfValue::Struct(set)) => states_cooldown(Some(set)),
                    _ => false,
                };
                let stated = own(contract) || structs(contract, "subContracts").any(|sub| states_cooldown(Some(sub)));
                if stated && let Some(DfValue::String(debug) | DfValue::Enum(debug)) = contract.field("debugName") {
                    found.insert((tree.record_path().to_string(), (*debug).to_string()));
                }
            }
        }
    }
    Ok(found)
}

fn compare_cooldowns(
    report: &mut Report,
    dcb: &[u8],
    texts: &Localization,
    build: Option<&str>,
    catalog: &Value,
    spoil: bool,
) -> Result<(), String> {
    let facts = ContractFacts::from_catalog(&serde_json::to_vec(catalog).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let blueprints = blueprints_from_dataforge(dcb, &facts, texts, &[], build).map_err(|e| e.to_string())?;
    let own = contracts_with_own_cooldown(dcb)?;
    let mut theirs: BTreeMap<(String, String), Vec<Option<i64>>> = BTreeMap::new();
    for contract in items(catalog, "contracts") {
        if contract["abandonedCooldownTime"].as_f64().is_some() {
            report.abandoned += 1;
        }
        let key = (
            contract["recordPath"].as_str().unwrap_or_default().to_string(),
            contract["debugName"].as_str().unwrap_or_default().to_string(),
        );
        theirs.entry(key).or_default().push(catalog_cooldown(contract));
    }
    let mut seen = BTreeSet::new();
    let mut first = true;
    for blueprint in &blueprints.blueprints {
        for source in &blueprint.sources {
            let key = (source.record_path.clone(), source.id.clone());
            if !seen.insert(key.clone()) {
                continue;
            }
            let Some(rows) = theirs.get(&key) else {
                report.cooldown_without_contract += 1;
                continue;
            };
            let stated = own.contains(&key);
            let mut ours = source.cooldown;
            if spoil && first && ours.is_some() && !stated {
                ours = ours.map(|c| c + 1);
                first = false;
            }
            let same = rows.iter().all(|row| *row == ours);
            if !same && stated {
                // VerseKit reads the parameter sets of the contract first, the catalog the handler only.
                report.cooldown_own += 1;
            } else {
                report.cooldowns.add(same, || format!("{}: {:?} gegen {:?}", source.id, ours, rows));
            }
        }
    }
    Ok(())
}

fn compare_elements(
    report: &mut Report,
    rocks: &[Rock],
    mining_file: &str,
    build: Option<&str>,
    spoil: bool,
) -> Result<(), String> {
    let work = versekit_core::testdir::fresh("parity-felsen");
    let mut text = std::fs::read_to_string(mining_file).map_err(|e| format!("{mining_file}: {e}"))?;
    if spoil {
        // One scan signature of the file differs from the game data by one.
        let mut data: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let element = data["elemente"]
            .as_object_mut()
            .and_then(|elements| elements.values_mut().find(|e| e["scanSignature"].as_i64().is_some()))
            .ok_or("keine Scan-Signatur in der Datei")?;
        element["scanSignature"] = Value::from(element["scanSignature"].as_i64().unwrap_or(0) + 1);
        text = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    }
    let before_file = work.join("vorher");
    let after_file = work.join("nachher");
    for folder in [&before_file, &after_file] {
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        std::fs::write(folder.join(mining_data::FILE), &text).map_err(|e| e.to_string())?;
    }
    mineables::write_signatures(&after_file.join(mineables::SIGNATURES_FILE).to_string_lossy(), rocks, build, None)
        .map_err(|e| e.to_string())?;
    let before = MiningData::load(&before_file.join(mining_data::FILE).to_string_lossy());
    let after = MiningData::load(&after_file.join(mining_data::FILE).to_string_lossy());
    for (old, new) in before.elements().iter().zip(after.elements()) {
        if old.signature == 0.0 {
            continue;
        }
        report.elements.add(old.signature == new.signature, || {
            format!("{}: Datei {} gegen Spieldaten {}", old.name, old.signature, new.signature)
        });
        if old.signature != new.signature {
            report.replaced.push(old.name.clone());
        }
    }
    report.asteroids = after.deposits().into_iter().skip(before.deposits().len()).map(|(n, s, _)| (n, s)).collect();
    let old_values: BTreeSet<u64> = signature_scan::possible_values(&before).into_iter().collect();
    let new_values: BTreeSet<u64> = signature_scan::possible_values(&after).into_iter().collect();
    report.new_values = new_values.difference(&old_values).copied().collect();
    report.lost_values = old_values.difference(&new_values).copied().collect();
    Ok(())
}

pub fn run(
    dcb_file: &str,
    ini_file: &str,
    catalog_file: &str,
    mining_file: &str,
    gegenprobe: bool,
) -> Result<(), String> {
    let dcb = std::fs::read(dcb_file).map_err(|e| format!("{dcb_file}: {e}"))?;
    let ini = std::fs::read(ini_file).map_err(|e| format!("{ini_file}: {e}"))?;
    let texts = Localization::from_files(Language::English, &ini, None).map_err(|e| e.to_string())?;
    let catalog = read(catalog_file)?;
    let build = catalog["source"]["build"]["version"].as_str().map(str::to_string);
    println!("Katalog: Stand {build:?}");
    let mut rocks = mineables::rocks_from_dataforge(&dcb, &texts).map_err(|e| e.to_string())?;
    let mut crafters = mineables::crafters_from_dataforge(&dcb).map_err(|e| e.to_string())?;
    println!(
        "gelesen: {} Steine ({} mit Scan-Signatur), {} Werkbänke",
        rocks.len(),
        rocks.iter().filter(|r| r.scan_signature.is_some()).count(),
        crafters.len()
    );
    if gegenprobe {
        if let Some(rock) = rocks.iter_mut().find(|r| r.scan_signature.is_some()) {
            rock.scan_signature = rock.scan_signature.map(|s| s + 1.0);
        }
        if let Some(queue) = crafters.iter_mut().find_map(|c| c.queues.first_mut()) {
            queue.max_jobs_waiting = queue.max_jobs_waiting.map(|n| n + 1);
        }
    }
    let mut report = Report::default();
    compare_rocks(&mut report, &rocks, &catalog);
    compare_crafters(&mut report, &crafters, &catalog);
    compare_cooldowns(&mut report, &dcb, &texts, build.as_deref(), &catalog, gegenprobe)?;
    // The file of the signatures is made from the rocks read, the ones of the check included.
    compare_elements(&mut report, &rocks, mining_file, build.as_deref(), gegenprobe)?;
    print!("{}", report.text());
    if gegenprobe {
        let missed: Vec<&str> = [
            ("Steine", report.rocks.different),
            ("Werkbänke", report.crafters.different),
            ("Sperrzeit", report.cooldowns.different),
            ("Rohstoffe", report.elements.different),
        ]
        .into_iter()
        .filter(|(_, different)| *different == 0)
        .map(|(name, _)| name)
        .collect();
        if !missed.is_empty() {
            return Err(format!("Gegenprobe hat nicht angeschlagen bei: {}", missed.join(", ")));
        }
        return Ok(());
    }
    if report.differences() > 0 {
        return Err(format!("{} Abweichungen", report.differences()));
    }
    println!("ERGEBNIS gleich");
    Ok(())
}
