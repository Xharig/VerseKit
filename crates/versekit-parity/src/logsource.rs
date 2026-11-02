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

//! Rust side of the parity check for backlog reading, read position, accounts
//! and live tailing.
//!
//! Replays the scenarios from the output of `tools/parity_logsource.py` in
//! the `rs` folders, with the same simulated clock and the same
//! modification times. Writing happens only in the scratch folder: the
//! write guard lets nothing else through and records every attempt.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use regex::Regex;
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::legacy;
use versekit_core::logscan::Find;
use versekit_core::logsource::{LogSource, LogTail, ReadState, Report};
use versekit_core::paths::{ENV_INSTALL_DIR, Host, Paths};
use versekit_core::phrases::pattern;
use versekit_core::pypath::Flavor;
use versekit_core::{errors, writeguard};

use crate::paths::sha256_hex;

static ERRORS: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub(crate) fn s(text: &str) -> Value {
    Value::str(text)
}

fn opt(value: Option<String>) -> Value {
    value.map_or(Value::Null, Value::Str)
}

pub(crate) fn int(n: impl Into<i128>) -> Value {
    Value::Int(n.into().to_string())
}

fn finds_json(finds: &[Find]) -> Value {
    Value::Array(
        finds
            .iter()
            .map(|(name, extra)| {
                let extra = match extra {
                    None => Value::Null,
                    Some((a, b, c)) => Value::Array(vec![s(a), opt(b.clone()), opt(c.clone())]),
                };
                Value::Array(vec![s(name), extra])
            })
            .collect(),
    )
}

fn report_json(r: &Report) -> Value {
    let reason = match &r.gap {
        None => s(""),
        Some(gap) => {
            let mut parts = vec![s(gap.key())];
            parts.extend(gap.moments().into_iter().map(Value::Float));
            Value::Array(parts)
        }
    };
    let count = |n: usize| int(n as i64);
    Value::Object(vec![
        ("dateien".into(), count(r.files)),
        ("uebersprungen".into(), count(r.skipped)),
        ("gefunden".into(), count(r.found)),
        ("vorhanden".into(), count(r.available)),
        ("luecke".into(), Value::Bool(r.gap.is_some())),
        ("grund".into(), reason),
        ("laufende".into(), Value::Bool(r.running)),
        ("unlesbar".into(), count(r.unreadable)),
        ("fremd".into(), count(r.foreign)),
    ])
}

fn backlog_json((finds, report): (Vec<Find>, Report)) -> Value {
    Value::Object(vec![("finds".into(), finds_json(&finds)), ("report".into(), report_json(&report))])
}

/// Replace the side folder with a fixed marker, like `neutral` on the Python side.
fn neutral(data: &[u8], prefix: &str) -> Vec<u8> {
    let quoted = jsonio::dumps(&s(prefix), None, false);
    let escaped = &quoted[1..quoted.len() - 1];
    let mut out = data.to_vec();
    for old in [prefix, escaped] {
        out = replace_bytes(&out, old.as_bytes(), b"<SEITE>/");
    }
    out
}

fn replace_bytes(data: &[u8], old: &[u8], new: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if !old.is_empty() && data[i..].starts_with(old) {
            out.extend_from_slice(new);
            i += old.len();
        } else {
            out.push(data[i]);
            i += 1;
        }
    }
    out
}

pub(crate) fn digest(path: &str, prefix: &str) -> Value {
    fs::read(path).map_or(Value::Null, |data| s(&sha256_hex(&neutral(&data, prefix))))
}

pub(crate) fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value, String> {
    value.get(key).ok_or_else(|| format!("Feld fehlt: {key}"))
}

pub(crate) fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    field(value, key)?.as_str().ok_or_else(|| format!("{key} ist kein Text"))
}

pub(crate) fn number(value: &Value, key: &str) -> Result<f64, String> {
    match field(value, key)? {
        Value::Int(t) => t.parse().map_err(|_| format!("{key} ist keine Zahl")),
        Value::Float(x) => Ok(*x),
        _ => Err(format!("{key} ist keine Zahl")),
    }
}

fn whole(value: &Value, key: &str) -> Result<i128, String> {
    match field(value, key)? {
        Value::Int(t) => t.parse().map_err(|_| format!("{key} ist keine Ganzzahl")),
        _ => Err(format!("{key} ist keine Ganzzahl")),
    }
}

/// Set the modification time to a fixed value, like `os.utime(path, ns=(t, t))`.
pub(crate) fn set_mtime(path: &str, ns: i128) -> Result<(), String> {
    writeguard::check("utime", path).map_err(|e| e.to_string())?;
    let at = UNIX_EPOCH + Duration::from_nanos(u64::try_from(ns).map_err(|_| "Zeit vor 1970")?);
    let file = fs::File::options().write(true).open(path).map_err(|e| e.to_string())?;
    file.set_times(fs::FileTimes::new().set_modified(at).set_accessed(at)).map_err(|e| e.to_string())
}

fn write_part(target: &str, append: bool, src: &str, start: u64, end: u64) -> Result<(), String> {
    let mut data = vec![0; usize::try_from(end - start).map_err(|_| "zu groß")?];
    let mut file = fs::File::open(src).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    file.read_exact(&mut data).map_err(|e| e.to_string())?;
    writeguard::check(if append { "open-ab" } else { "open-wb" }, target).map_err(|e| e.to_string())?;
    let mut out = fs::File::options()
        .write(true)
        .create(true)
        .append(append)
        .truncate(!append)
        .open(target)
        .map_err(|e| e.to_string())?;
    out.write_all(&data).map_err(|e| e.to_string())
}

/// The simulated clock: start, step per query, counter, surcharge.
#[derive(Clone)]
pub(crate) struct FakeClock {
    pub(crate) base: f64,
    pub(crate) step: f64,
    pub(crate) state: Arc<Mutex<(u64, f64)>>,
}

impl FakeClock {
    pub(crate) fn boxed(&self) -> versekit_core::logsource::Clock {
        let me = self.clone();
        Box::new(move || {
            let mut st = me.state.lock().unwrap_or_else(|e| e.into_inner());
            let value = me.base + me.step * st.0 as f64 + st.1;
            st.0 += 1;
            value
        })
    }

    pub(crate) fn add(&self, seconds: f64) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).1 += seconds;
    }
}

/// A program start: fresh paths and caches. Deliberately never freed
/// (a check run has only a handful of them).
fn start(home: &str, game: &str, clock: &FakeClock) -> &'static LogSource<'static> {
    let mut host = Host::current();
    host.set_var(legacy::ENV_HOME, home);
    host.set_var(ENV_INSTALL_DIR, game);
    let paths: &'static Paths = Box::leak(Box::new(Paths::new(host)));
    let sources = LogSource::sources_of(paths);
    Box::leak(Box::new(LogSource::with_clock(paths, sources, clock.boxed())))
}

struct Run<'a> {
    scratch: &'a str,
    real_game: &'a str,
    mtime: (i128, i128),
    clock: FakeClock,
    gegenprobe: bool,
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
        let mut src = start(&home, &game, &self.clock);
        let (phrases, _) = src.sources.collect();
        let rx: Regex = pattern(&phrases);
        let mut tail: Option<LogTail<'static, 'static>> = None;
        let mut results = Vec::new();
        let mut tails = 0;
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
                "backlog" => {
                    let mut state = ReadState::open(src.paths);
                    backlog_json(src.read_backlog(&mut state, &rx, flag("only_new"), flag("incl_running")))
                }
                "read_all" => backlog_json(src.read_all(&rx).map_err(|e| e.to_string())?),
                "accounts" => Value::Array(
                    src.accounts_in_logs(None)
                        .into_iter()
                        .map(|(a, n)| Value::Array(vec![s(&a), int(n as i64)]))
                        .collect(),
                ),
                "own" => opt(src.own_account(None)),
                "foreign" => Value::Array(src.foreign_only_blueprints(&rx, None).iter().map(|n| s(n)).collect()),
                "setting" => Value::Bool(src.paths.set_setting(text(op, "key")?, s(text(op, "value")?))),
                "clock" => {
                    self.clock.add(number(op, "add")?);
                    Value::Null
                }
                "restart" => {
                    tail = None;
                    src = start(&home, &game, &self.clock);
                    Value::Null
                }
                "tail_start" => {
                    tail = Some(LogTail::new(src, ReadState::open(src.paths), rx.clone()));
                    Value::Null
                }
                "tail" => {
                    let t = tail.as_mut().ok_or("Mitlesen nicht gestartet")?;
                    let mut names = finds_json(&t.new_names());
                    tails += 1;
                    if self.gegenprobe
                        && name == "mitlesen"
                        && tails == 6
                        && let Value::Array(items) = &mut names
                    {
                        items.push(Value::Array(vec![s("Gegenprobe"), Value::Null]));
                    }
                    Value::Object(vec![
                        ("names".into(), names),
                        ("account".into(), opt(t.account.clone())),
                        ("offset".into(), int(t.offset)),
                        ("path".into(), opt(t.path.clone())),
                    ])
                }
                "write" => {
                    let start_at = whole(op, "start")?;
                    let end_at = whole(op, "end")?;
                    let range = (u64::try_from(start_at), u64::try_from(end_at));
                    let (Ok(a), Ok(b)) = range else { return Err("Bereich ungültig".into()) };
                    write_part(&game_log, text(op, "mode")? == "append", text(op, "src")?, a, b)?;
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
            let state = digest(&f.join(&home, "logstand.json"), &prefix);
            results.push(Value::Object(vec![
                ("op".into(), s(kind)),
                ("result".into(), result),
                ("logstand".into(), state),
            ]));
        }
        Ok(Value::Object(vec![("name".into(), s(name)), ("results".into(), Value::Array(results))]))
    }
}

/// Deliberately wrong: one byte in a scenario's read position.
fn sabotage(scratch: &str) {
    let path = Flavor::native().join_all(scratch, &["rs", "stand", "home", "logstand.json"]);
    if let Ok(mut bytes) = fs::read(&path)
        && let Some(b) = bytes.last_mut()
    {
        *b ^= 0x01;
        let _ = fs::write(&path, bytes);
    }
}

pub(crate) fn stat(path: &str) -> Value {
    match fs::metadata(path) {
        Ok(m) => {
            let ns =
                m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as i128);
            Value::Array(vec![int(m.len() as i64), int(ns)])
        }
        Err(_) => Value::Null,
    }
}

pub fn run(python_json: &str, out: &str, gegenprobe: bool) -> Result<(), String> {
    let input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let scratch = text(&input, "scratch")?.to_string();
    let real_game = text(&input, "real_game")?.to_string();
    let (clock, mtime) = match (field(&input, "clock")?, field(&input, "mtime")?) {
        (Value::Array(c), Value::Array(m)) if c.len() == 2 && m.len() == 2 => {
            let num = |v: &Value| match v {
                Value::Int(t) => t.parse::<f64>().ok(),
                Value::Float(x) => Some(*x),
                _ => None,
            };
            let ns = |v: &Value| match v {
                Value::Int(t) => t.parse::<i128>().ok(),
                _ => None,
            };
            let base = num(&c[0]).ok_or("clock ungültig")?;
            let step = num(&c[1]).ok_or("clock ungültig")?;
            let clock = FakeClock { base, step, state: Arc::new(Mutex::new((0, 0.0))) };
            (clock, (ns(&m[0]).ok_or("mtime ungültig")?, ns(&m[1]).ok_or("mtime ungültig")?))
        }
        _ => return Err("clock/mtime ungültig".into()),
    };
    errors::set_sink(|label, _| {
        if let Ok(mut list) = ERRORS.lock() {
            list.push(label.to_string());
        }
    });
    writeguard::deny(true);
    writeguard::allow_under(Some(&scratch));

    let runner = Run { scratch: &scratch, real_game: &real_game, mtime, clock, gegenprobe };
    let mut scenarios = Vec::new();
    for sc in match field(&input, "scenarios")? {
        Value::Array(items) => items,
        _ => return Err("scenarios ist keine Liste".into()),
    } {
        let result = runner.scenario(sc)?;
        eprintln!("{:<11} fertig", text(sc, "name")?);
        scenarios.push(result);
    }
    let blocked = writeguard::take_denied();
    writeguard::deny(false);
    writeguard::allow_under(None);
    if gegenprobe {
        sabotage(&scratch);
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
