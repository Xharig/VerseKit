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

//! Rust side of the parity check for the contracts backlog reading
//! (`mission_log.scan_backlog`, `reassess`) with the playtime and the
//! statistics it starts.
//!
//! Replays the scenarios from the output of `tools/parity_contracts.py` in the
//! `rs` folders, with the same simulated clocks and the same modification
//! times. Writing happens only in the scratch folder: the write guard lets
//! nothing else through and records every attempt.

use std::fs;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use versekit_core::jsonio::{self, Map, Value};
use versekit_core::legacy;
use versekit_core::logsource::LogSource;
use versekit_core::mission_scan::MissionScan;
use versekit_core::paths::{ENV_INSTALL_DIR, Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::{errors, writeguard};

use crate::logsource::{FakeClock, digest, field, int, number, s, set_mtime, stat, text};

static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The files of a data folder looked at after every step.
const WATCH: [&str; 6] = [
    "auftragslog.json",
    "auftragslog.bak.json",
    "spielzeit.json",
    "statistik.json",
    "konten-je-log.json",
    "einstellungen.json",
];

/// A program start: fresh paths and caches. Deliberately never freed (a check
/// run has only a handful of them).
fn start(
    home: &str,
    game: &str,
    clock: &FakeClock,
    stats: &FakeClock,
) -> (&'static LogSource<'static>, &'static MissionScan<'static, 'static>) {
    let mut host = Host::current();
    host.set_var(legacy::ENV_HOME, home);
    host.set_var(ENV_INSTALL_DIR, game);
    let paths: &'static Paths = Box::leak(Box::new(Paths::new(host)));
    let sources = LogSource::sources_of(paths);
    let src: &'static LogSource<'static> = Box::leak(Box::new(LogSource::with_clock(paths, sources, clock.boxed())));
    let scan: &'static MissionScan<'static, 'static> = Box::leak(Box::new(MissionScan::with_clock(src, stats.boxed())));
    (src, scan)
}

struct Run<'a> {
    scratch: &'a str,
    real_game: &'a str,
    mtime: (i128, i128),
    clock: FakeClock,
    stats: FakeClock,
    gegenprobe: bool,
}

fn clock_of(value: &Value, key: &str) -> Result<FakeClock, String> {
    match field(value, key)? {
        Value::Array(c) if c.len() == 2 => {
            let num = |v: &Value| match v {
                Value::Int(t) => t.parse::<f64>().ok(),
                Value::Float(x) => Some(*x),
                _ => None,
            };
            let base = num(&c[0]).ok_or(format!("{key} ungültig"))?;
            let step = num(&c[1]).ok_or(format!("{key} ungültig"))?;
            Ok(FakeClock { base, step, state: Arc::new(Mutex::new((0, 0.0))) })
        }
        _ => Err(format!("{key} ungültig")),
    }
}

impl Run<'_> {
    fn scenario(&self, sc: &Value) -> Result<Value, String> {
        let f = Flavor::native();
        let name = text(sc, "name")?;
        let base = f.join_all(self.scratch, &["rs", name]);
        let home = f.join(&base, "home");
        let game = if text(sc, "game")? == "real" {
            self.real_game.to_string()
        } else {
            f.join_all(&base, &["game", "StarCitizen", "LIVE"])
        };
        let game_log = f.join(&game, "Game.log");
        let prefix = format!("{}{}", f.join(self.scratch, "rs"), f.sep());
        let (mut src, mut scan) = start(&home, &game, &self.clock, &self.stats);
        let mut results = Vec::new();
        let ops = match field(sc, "ops")? {
            Value::Array(items) => items,
            _ => return Err("ops ist keine Liste".into()),
        };
        for (index, op) in ops.iter().enumerate() {
            if src.paths.game_folder().as_deref() != Some(game.as_str()) {
                return Err(format!("Spielordner ist nicht der Testordner ({name}) — Abbruch."));
            }
            let kind = text(op, "op")?;
            let stamp = self.mtime.0 + self.mtime.1 * index as i128;
            let flag = |key: &str| matches!(op.get(key), Some(Value::Bool(true)));
            let result = match kind {
                "scan" => match scan.scan_backlog() {
                    Ok((total, added)) => {
                        // Counter-check: one more new contract than there is.
                        let added = if self.gegenprobe && name == "voll" && index == 0 { added + 1 } else { added };
                        Value::Array(vec![int(total as i64), int(added as i64)])
                    }
                    Err(_) => s("FEHLER"),
                },
                "reassess" => {
                    let folder = flag("folder").then_some(game.as_str());
                    let running = flag("running").then_some(game_log.as_str());
                    match scan.reassess(folder, running) {
                        Ok((total, added, corrected)) => {
                            Value::Array(vec![int(total as i64), int(added as i64), int(corrected as i64)])
                        }
                        Err(_) => s("FEHLER"),
                    }
                }
                "setting" => {
                    let value = field(op, "value")?.clone();
                    Value::Bool(src.paths.set_setting(text(op, "key")?, value))
                }
                "restart" => {
                    (src, scan) = start(&home, &game, &self.clock, &self.stats);
                    Value::Null
                }
                "clock" => {
                    self.clock.add(number(op, "add")?);
                    Value::Null
                }
                "write" => {
                    writeguard::check("open-ab", &game_log).map_err(|e| e.to_string())?;
                    let mut out =
                        fs::File::options().append(true).create(true).open(&game_log).map_err(|e| e.to_string())?;
                    out.write_all(text(op, "text")?.as_bytes()).map_err(|e| e.to_string())?;
                    drop(out);
                    set_mtime(&game_log, stamp)?;
                    Value::Null
                }
                "rotate" => {
                    let target = f.join_all(&game, &["logbackups", text(op, "name")?]);
                    jsonio::os_replace(&game_log, &target).map_err(|e| e.to_string())?;
                    set_mtime(&target, stamp)?;
                    jsonio::write_new(&game_log, b"").map_err(|e| e.to_string())?;
                    set_mtime(&game_log, stamp)?;
                    Value::Null
                }
                other => return Err(format!("unbekannter Schritt {other}")),
            };
            let files: Map =
                WATCH.iter().map(|name| ((*name).to_string(), digest(&f.join(&home, name), &prefix))).collect();
            results.push(Value::Object(vec![
                ("op".into(), s(kind)),
                ("result".into(), result),
                ("dateien".into(), Value::Object(files)),
            ]));
        }
        Ok(Value::Object(vec![("name".into(), s(name)), ("results".into(), Value::Array(results))]))
    }
}

/// Puts the folders of the Rust side back to where the Python side left them
/// before the run (`rs-anfang`), so a run can be repeated.
fn restore(scratch: &str) -> Result<(), String> {
    fn copy_tree(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let target = to.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                copy_tree(&entry.path(), &target)?;
            } else {
                fs::copy(entry.path(), &target)?;
                let modified = entry.metadata()?.modified()?;
                fs::File::options()
                    .write(true)
                    .open(&target)?
                    .set_times(fs::FileTimes::new().set_modified(modified))?;
            }
        }
        Ok(())
    }
    let f = Flavor::native();
    let (start, target) = (f.join(scratch, "rs-anfang"), f.join(scratch, "rs"));
    if !std::path::Path::new(&start).is_dir() {
        return Ok(());
    }
    writeguard::check("restore", &target).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&target);
    copy_tree(std::path::Path::new(&start), std::path::Path::new(&target)).map_err(|e| e.to_string())
}

/// Deliberately wrong: one byte in a file of a scenario.
fn flip_last_byte(scratch: &str, scenario: &str, file: &str) {
    let path = Flavor::native().join_all(scratch, &["rs", scenario, "home", file]);
    if let Ok(mut bytes) = fs::read(&path)
        && let Some(b) = bytes.last_mut()
    {
        *b ^= 0x01;
        let _ = fs::write(&path, bytes);
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let scratch = text(&input, "scratch")?.to_string();
    let real_game = text(&input, "real_game")?.to_string();
    let clock = clock_of(&input, "clock")?;
    let stats = clock_of(&input, "stats_clock")?;
    let mtime = match field(&input, "mtime")? {
        Value::Array(m) if m.len() == 2 => {
            let ns = |v: &Value| match v {
                Value::Int(t) => t.parse::<i128>().ok(),
                _ => None,
            };
            (ns(&m[0]).ok_or("mtime ungültig")?, ns(&m[1]).ok_or("mtime ungültig")?)
        }
        _ => return Err("mtime ungültig".into()),
    };
    errors::set_sink(|label, _| {
        if let Ok(mut list) = ERRORS.lock() {
            list.push(label.to_string());
        }
    });
    writeguard::deny(true);
    writeguard::allow_under(Some(&scratch));
    restore(&scratch)?;

    let runner = Run { scratch: &scratch, real_game: &real_game, mtime, clock, stats, gegenprobe };
    let mut scenarios = Vec::new();
    for sc in match field(&input, "scenarios")? {
        Value::Array(items) => items,
        _ => return Err("scenarios ist keine Liste".into()),
    } {
        let started = Instant::now();
        let result = runner.scenario(sc)?;
        eprintln!("{:<11} fertig in {:.2?}", text(sc, "name")?, started.elapsed());
        scenarios.push(result);
    }
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);
    if gegenprobe {
        flip_last_byte(&scratch, "stand", "auftragslog.json");
        flip_last_byte(&scratch, "stand", "spielzeit.json");
        flip_last_byte(&scratch, "teil", "statistik.json");
        flip_last_byte(&scratch, "synth", "konten-je-log.json");
    }
    let errors = ERRORS.lock().map(|list| list.clone()).unwrap_or_default();
    let result: Map = vec![
        ("scenarios".into(), Value::Array(scenarios)),
        ("real_game_log".into(), stat(&Flavor::native().join(&real_game, "Game.log"))),
        ("blocked".into(), Value::Array(blocked.into_iter().map(Value::Str).collect())),
        ("errors".into(), Value::Array(errors.into_iter().map(Value::Str).collect())),
    ];
    fs::write(out, jsonio::dumps(&Value::Object(result), Some(1), false)).map_err(|e| e.to_string())
}
