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

//! Rust side of the parity check for the running contracts of the overlay
//! (`tools/parity_live.py`): the same steps on the same made-up game log, on
//! a copy of the real catalog and the `global.ini` of the game folder. Every
//! step gives the messages for the display and the file of the contracts
//! hidden by hand, byte for byte.
//!
//! `--gegenprobe` leaves out the clearing of the silent contracts at the end
//! of the game, so the comparison must report it.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::sync::Mutex;

use versekit_core::catalog_watch::{Item, Watch};
use versekit_core::contracts::Patterns;
use versekit_core::errors;
use versekit_core::jsonio::{self, Value};
use versekit_core::language::{self, Choice};
use versekit_core::legacy::ENV_HOME;
use versekit_core::live_contracts::{Check, Live, Out, Section, SectionScan, hidden_load};
use versekit_core::logsource::ContractScan;
use versekit_core::mission_log::{ContractBlueprints, clean, ini_files};
use versekit_core::paths::{ENV_INSTALL_DIR, Host, Paths, name_key};
use versekit_core::refinery_jobs;
use versekit_core::text::decode_ignore;
use versekit_core::watchlist;
use versekit_core::writeguard;

static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn obj(pairs: Vec<(&str, Value)>) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn text(value: &Value, key: &str) -> Result<String, String> {
    value.get(key).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("{key} fehlt"))
}

fn strs(items: &[String]) -> Value {
    Value::Array(items.iter().map(|s| Value::str(s.as_str())).collect())
}

fn out_value(out: &Out) -> Value {
    match out {
        Out::Stand(lines) => obj(vec![
            ("k", Value::str("stand")),
            (
                "lines",
                Value::Array(
                    lines
                        .iter()
                        .map(|l| {
                            Value::Array(vec![
                                Value::str(l.title.as_str()),
                                Value::str(l.text.render()),
                                strs(&l.goals),
                                Value::Bool(l.grey),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]),
        Out::Gone(title) => obj(vec![("k", Value::str("weg")), ("t", Value::str(title.as_str()))]),
        Out::Hint { text, contract } => obj(vec![
            ("k", Value::str("hinweis")),
            ("text", Value::str(text.render())),
            ("t", Value::str(contract.as_str())),
        ]),
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let home = text(&input, "rust_home")?;
    let game = text(&input, "rust_game")?;
    errors::set_sink(|label, _| {
        if let Ok(mut list) = ERRORS.lock() {
            list.push(label.to_string());
        }
    });
    writeguard::deny(true);
    writeguard::allow_under(Some(&home));
    let result = run_in(&input, &home, &game, gegenprobe, out);
    writeguard::deny(false);
    writeguard::allow_under(None);
    result
}

fn run_in(input: &Value, home: &str, game: &str, gegenprobe: bool, out: &str) -> Result<(), String> {
    let mut host = Host::current();
    host.registry = false;
    host.set_var(ENV_HOME, home);
    host.set_var(ENV_INSTALL_DIR, game);
    let paths = Paths::new(host);
    let Some(Value::Array(scenarios)) = input.get("scenarios") else { return Err("scenarios fehlt".into()) };
    let mut done: Vec<Value> = Vec::new();
    for scenario in scenarios {
        let name = text(scenario, "name")?;
        let Some(Value::Array(ops)) = scenario.get("ops") else { return Err("ops fehlt".into()) };
        let _ = fs::remove_file(paths.app_file(versekit_core::live_contracts::HIDDEN_FILE));
        language::set_language(Choice::De);
        let blueprints = ContractBlueprints::load(&paths);
        let patterns = Patterns::build(&ini_files(&paths));
        let mut live = Live::new();
        let mut watch = Watch::default();
        for stale in [watchlist::FILE, refinery_jobs::FILE, versekit_core::catalog_watch::SEEN_FILE] {
            let _ = fs::remove_file(paths.app_file(stale));
        }
        let owned: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
        let has = |n: &str| owned.borrow().contains(&name_key(n));
        let mut results: Vec<Value> = Vec::new();
        for op in ops {
            let kind = text(op, "op")?;
            let mut messages: Vec<Out> = Vec::new();
            let mut value = Value::Null;
            let mut extra: Vec<Value> = Vec::new();
            if matches!(kind.as_str(), "start" | "rescan")
                && let Some(log) = op.get("log").and_then(Value::as_str)
            {
                fs::write(format!("{game}/Game.log"), log).map_err(|e| e.to_string())?;
            }
            match kind.as_str() {
                "start" => at_start(&paths, &mut live, &patterns, &blueprints, &has, &mut messages),
                "section" => {
                    let text = text(op, "text")?;
                    let (mut scan, shared) = SectionScan::new(patterns.clone());
                    scan.scan(&text);
                    let section: Section = SectionScan::taken(&shared);
                    let check = Check { blueprints: &blueprints, has: &has };
                    live.report(&paths, &section, &text, &check, &mut messages);
                }
                "hide" => {
                    let title = clean(&text(op, "title")?);
                    live.hide(&paths, &title, &mut messages);
                }
                "game_end" => {
                    live.clear_unclear = !gegenprobe;
                    live.silence(None, "", &mut messages);
                }
                "rescan" => {
                    let before = live.reset();
                    at_start(&paths, &mut live, &patterns, &blueprints, &has, &mut messages);
                    live.after_reset(&before, &mut messages);
                }
                "hidden_file" => {
                    let target = paths.app_file(versekit_core::live_contracts::HIDDEN_FILE);
                    match op.get("text").and_then(Value::as_str) {
                        Some(raw) => fs::write(&target, raw).map_err(|e| e.to_string())?,
                        None => {
                            let _ = fs::remove_file(&target);
                        }
                    }
                }
                "watch_file" | "refinery_file" => {
                    let name = if kind == "watch_file" { watchlist::FILE } else { refinery_jobs::FILE };
                    let target = paths.app_file(name);
                    let _ = jsonio::make_dirs(&versekit_core::pypath::Flavor::native().dirname(&target));
                    // Counter check: one pattern of the list is spelt another way.
                    let content = text(op, "text")?;
                    let content = if gegenprobe { content.replace("woodland", "woodlands") } else { content };
                    fs::write(&target, content).map_err(|e| e.to_string())?;
                }
                "fulfill" => {
                    value = watchlist::fulfill(&paths, &text(op, "name")?).map_or(Value::Null, Value::Str);
                }
                "refinery" => {
                    let now = match op.get("now") {
                        Some(Value::Float(x)) => *x,
                        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
                        _ => 0.0,
                    };
                    // Counter check: the clock of the refinery runs a little slow.
                    let now = if gegenprobe { now - 20.0 } else { now };
                    let labels: Vec<Value> = refinery_jobs::announce_due(&paths, now)
                        .iter()
                        .map(|j| Value::Str(refinery_jobs::job_label(j, 2)))
                        .collect();
                    value = Value::Array(labels);
                }
                "catalog" => {
                    let items: Vec<Item> = match op.get("items") {
                        Some(Value::Array(list)) => list
                            .iter()
                            .map(|i| Item {
                                key: text(i, "key").unwrap_or_default(),
                                name: text(i, "name").unwrap_or_default(),
                                kind: text(i, "art").unwrap_or_default(),
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    // The clock of the Python side stands still at this moment.
                    for found in watch.look(&paths, &text(op, "version")?, &items, "2026-10-02 12:34:56") {
                        extra.push(obj(vec![
                            ("k", Value::str("katalog")),
                            ("name", Value::Str(found.name)),
                            ("art", Value::Str(found.kind)),
                            ("zeit", Value::str("12:34:56")),
                            ("titel", found.title.map_or(Value::Null, Value::Str)),
                        ]));
                    }
                }
                "bestand" => {
                    *owned.borrow_mut() = match op.get("names") {
                        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(name_key).collect(),
                        _ => HashSet::new(),
                    };
                }
                "language" => {
                    let lang = text(op, "lang")?;
                    language::set_language(if lang == "en" { Choice::En } else { Choice::De });
                }
                other => return Err(format!("unbekannter Schritt {other}")),
            }
            // A file as the bytes it holds, one character per byte.
            let bytes_of = |name: &str| {
                fs::read(paths.app_file(name))
                    .map_or(Value::Null, |bytes| Value::Str(bytes.iter().map(|b| char::from(*b)).collect()))
            };
            let mut out: Vec<Value> = messages.iter().map(out_value).collect();
            out.extend(extra);
            results.push(obj(vec![
                ("op", Value::str(kind.as_str())),
                ("out", Value::Array(out)),
                ("offen", strs(&live.titles())),
                ("hidden", bytes_of(versekit_core::live_contracts::HIDDEN_FILE)),
                ("wert", value),
                (
                    "dateien",
                    obj(vec![
                        ("merkliste", bytes_of(watchlist::FILE)),
                        ("raffinerie", bytes_of(refinery_jobs::FILE)),
                        ("katalog", bytes_of(versekit_core::catalog_watch::SEEN_FILE)),
                    ]),
                ),
            ]));
        }
        language::set_language(Choice::De);
        done.push(obj(vec![("name", Value::str(name.as_str())), ("results", Value::Array(results))]));
    }
    let errors = ERRORS.lock().map(|list| list.clone()).unwrap_or_default();
    let result = obj(vec![("scenarios", Value::Array(done)), ("errors", strs(&errors))]);
    // Not through the guarded write: the output lies outside the folder of the run.
    fs::write(out, jsonio::dumps(&result, Some(1), false)).map_err(|e| e.to_string())
}

fn at_start(
    paths: &Paths,
    live: &mut Live,
    patterns: &Patterns,
    blueprints: &ContractBlueprints,
    has: &dyn Fn(&str) -> bool,
    messages: &mut Vec<Out>,
) {
    let Some(file) = paths.game_log(None) else { return };
    let Ok(bytes) = fs::read(&file) else { return };
    let log = decode_ignore(&bytes);
    let check = Check { blueprints, has };
    live.at_start(&log, patterns, &check, &hidden_load(paths), messages);
}
