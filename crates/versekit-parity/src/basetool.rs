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

//! Rust side of the parity check for the Basetool connection
//! (`tools/parity_basetool.py`): the same pure functions, the secrets of the
//! Python side read and own ones written, and the protocol scenario against
//! the same made-up Basetool, which this side starts itself.

use std::collections::HashSet;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use versekit_core::basetool::config::{self, Config};
use versekit_core::basetool::connection::{Connection, DeviceLogin, new_idempotency_key, system_clock, urlencode};
use versekit_core::basetool::error::{ApiError, backoff, parse_retry_after};
use versekit_core::basetool::hold::{self, ChangeSet, Confirmations};
use versekit_core::basetool::http::HttpTransport;
use versekit_core::basetool::jose::{self, Jwk};
use versekit_core::basetool::key::{self, KEY_SECRET, KeyVault};
use versekit_core::basetool::orgunits;
use versekit_core::basetool::proof::{ServerClock, asks_for_nonce};
use versekit_core::basetool::schedule::Schedule;
use versekit_core::basetool::scopes;
use versekit_core::basetool::secrets::{REFRESH_SECRET, SecretStore, SystemSecrets};
use versekit_core::basetool::state;
use versekit_core::basetool::sync::{Phase, Setup, SyncHandle};
use versekit_core::jsonio;
use versekit_core::legacy;
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::version;

fn to_core(value: &Value) -> jsonio::Value {
    jsonio::loads(&value.to_string()).unwrap_or(jsonio::Value::Null)
}

fn to_serde(value: &jsonio::Value) -> Value {
    serde_json::from_str(&jsonio::dumps(value, None, false)).unwrap_or(Value::Null)
}

fn map_to_serde(map: &jsonio::Map) -> Value {
    to_serde(&jsonio::Value::Object(map.clone()))
}

fn items<'a>(input: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    input[key].as_array().ok_or_else(|| format!("{key} fehlt"))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key].as_str().ok_or_else(|| format!("{key} fehlt"))
}

fn jwk_json(jwk: &Jwk) -> Value {
    json!({"kty": "EC", "crv": "P-256", "x": jwk.x, "y": jwk.y})
}

fn host(vars: &[(&str, &str)]) -> Host {
    Host::new(Flavor::native(), vars, "/")
}

// ------------------------------------------------------------- part A

fn part_a(input: &Value, core_input: &jsonio::Value, out: &mut serde_json::Map<String, Value>) -> Result<(), String> {
    let mut htu = Vec::new();
    for case in items(input, "htu")? {
        let url = text(case, "url")?;
        let result = match jose::normalize_htu(url) {
            Ok(value) => json!({"ok": value}),
            Err(_) => json!({"error": "ValueError"}),
        };
        htu.push(json!({"url": url, "result": result}));
    }
    out.insert("htu".into(), Value::Array(htu));

    let mut b64 = Vec::new();
    for case in items(input, "b64")? {
        let t = text(case, "text")?;
        let result = match jose::b64url_decode(t) {
            Ok(bytes) => json!({"ok": bytes}),
            Err(_) => json!({"error": "ValueError"}),
        };
        b64.push(json!({"text": t, "result": result}));
    }
    out.insert("b64".into(), Value::Array(b64));

    let mut ath = Vec::new();
    for case in items(input, "ath")? {
        let token = text(case, "token")?;
        ath.push(json!({"token": token, "ath": jose::access_token_hash(token)}));
    }
    out.insert("ath".into(), Value::Array(ath));
    let jwk = &input["thumbprint"]["jwk"];
    let key = Jwk { x: text(jwk, "x")?.into(), y: text(jwk, "y")?.into() };
    out.insert("thumbprint".into(), json!({"jwk": jwk_json(&key), "value": key.thumbprint()}));

    let mut errors = Vec::new();
    for case in items(input, "errors")? {
        let code = text(case, "code")?;
        let status = case["status"].as_u64().unwrap_or(0) as u16;
        let mut error = ApiError::new(code, status).with_retry_after(case["retry_after"].as_u64());
        if status != 0 {
            let detail = if status == 400 { "x".repeat(170) } else { "Ä kurz".to_string() };
            error = error.with_problem(vec![
                ("detail".into(), jsonio::Value::str(detail)),
                ("correlationId".into(), jsonio::Value::int(7)),
                ("confirmationUrl".into(), jsonio::Value::str("geheim")),
            ]);
        }
        if status == 409 {
            error.path = "POST /org-stock/changes".into();
        }
        errors.push(json!({
            "code": code, "status": status, "retry_after": error.retry_after,
            "detail": error.problem_text("detail"), "path": error.path,
            "action": error.action().name(), "describe": error.describe(), "final_code": error.code,
        }));
    }
    out.insert("errors".into(), Value::Array(errors));

    let mut waits = Vec::new();
    for case in items(input, "backoff")? {
        let attempt = case["attempt"].as_u64().unwrap_or(0) as u32;
        let retry_after = case["retry_after"].as_u64();
        let jitter = case["jitter"].as_f64().unwrap_or(0.0);
        waits.push(json!({"attempt": attempt, "retry_after": retry_after, "jitter": jitter,
                          "wait": backoff(attempt, retry_after, jitter)}));
    }
    out.insert("backoff".into(), Value::Array(waits));
    let mut retry = Vec::new();
    for case in items(input, "retry_after")? {
        retry.push(json!({"value": case["value"], "result": parse_retry_after(case["value"].as_str())}));
    }
    out.insert("retry_after".into(), Value::Array(retry));

    let trusted_inputs: Vec<&str> = items(input, "trusted_inputs")?.iter().filter_map(Value::as_str).collect();
    let mut configs = Vec::new();
    for case in items(input, "configs")? {
        let env = case["env"].as_object().ok_or("env fehlt")?;
        let pairs: Vec<(&str, &str)> = env.iter().filter_map(|(k, v)| Some((k.as_str(), v.as_str()?))).collect();
        let config = Config::from_host(&host(&pairs));
        configs.push(json!({
            "env": env,
            "config": {"issuer": config.issuer, "api": config.api, "client_id": config.client_id,
                       "ca": config.ca, "sandbox": config.sandbox, "override": config.overridden},
            "trusted": trusted_inputs.iter().map(|u| config.trusted_address(u)).collect::<Vec<_>>(),
        }));
    }
    out.insert("configs".into(), Value::Array(configs));

    let labels: Vec<Value> = items(input, "labels")?
        .iter()
        .map(|c| json!({"label": c["label"], "valid": config::label_valid(c["label"].as_str().unwrap_or_default())}))
        .collect();
    out.insert("labels".into(), Value::Array(labels));
    let versions: Vec<Value> = items(input, "versions")?
        .iter()
        .map(|c| {
            let (a, b) = (c["other"].as_str().unwrap_or_default(), c["own"].as_str().unwrap_or_default());
            json!({"other": a, "own": b, "newer": version::is_newer(a, b)})
        })
        .collect();
    out.insert("versions".into(), Value::Array(versions));

    let mut clocks = Vec::new();
    for case in items(input, "clock")? {
        let date = text(case, "date")?;
        let clock = ServerClock::default();
        let headers = if date.is_empty() { vec![] } else { vec![("Date".to_string(), date.to_string())] };
        clock.learn("https://login.example/token", &headers, 1000.25);
        clocks.push(json!({"date": date, "same": clock.now("https://LOGIN.example/x", 1000.25),
                           "other": clock.now("https://api.example/", 1000.25)}));
    }
    out.insert("clock".into(), Value::Array(clocks));

    let mut nonce = Vec::new();
    for case in items(input, "nonce")? {
        let headers: Vec<(String, String)> = case["headers"]
            .as_object()
            .map(|h| h.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect())
            .unwrap_or_default();
        let status = case["status"].as_u64().unwrap_or(0) as u16;
        nonce.push(json!({"status": status, "headers": case["headers"], "error": case["error"],
                          "asks": asks_for_nonce(status, &headers, case["error"].as_str())}));
    }
    out.insert("nonce".into(), Value::Array(nonce));

    let mut forms = Vec::new();
    for case in items(input, "forms")? {
        let pairs: Vec<(String, String)> = case["pairs"]
            .as_array()
            .map(|p| {
                p.iter()
                    .map(|pair| {
                        (pair[0].as_str().unwrap_or_default().into(), pair[1].as_str().unwrap_or_default().into())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let borrowed: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        forms.push(json!({"pairs": case["pairs"], "encoded": urlencode(&borrowed)}));
    }
    out.insert("forms".into(), Value::Array(forms));

    let answer = match to_core(&input["units"]["answer"]) {
        jsonio::Value::Object(map) => map,
        _ => return Err("units fehlt".into()),
    };
    let units = orgunits::parse(&answer);
    out.insert(
        "units".into(),
        json!({
            "answer": input["units"]["answer"],
            "units": units.iter().map(|u| json!({"id": u.id, "name": u.name, "shorthand": u.shorthand, "kind": u.kind}))
                .collect::<Vec<_>>(),
            "labels": units.iter().map(orgunits::label).collect::<Vec<_>>(),
        }),
    );

    // Read with the core's reader: serde_json would turn 10**20 into a float.
    let ops: Vec<jsonio::Value> = match core_input.get("ops_input") {
        Some(jsonio::Value::Array(list)) => list.clone(),
        _ => return Err("ops_input fehlt".into()),
    };
    let set = |range: &[jsonio::Value], keys: &[&str]| -> ChangeSet {
        (
            jsonio::Value::Object(vec![("ops".into(), jsonio::Value::Array(range.to_vec()))]),
            keys.iter().map(|k| (*k).to_string()).collect(),
        )
    };
    let fingerprints = vec![
        hold::fingerprint(&[set(&ops[..2], &["a", "a"]), set(&ops[2..], &["b", "c"])], &[]),
        hold::fingerprint(&[set(&ops, &["a", "a", "b", "c"])], &[]),
        hold::fingerprint(&[set(&ops, &[])], &[hold::op_hash(&ops[2])]),
        hold::fingerprint(&[], &[]),
    ];
    out.insert(
        "ops".into(),
        json!({"hashes": ops.iter().map(hold::op_hash).collect::<Vec<_>>(), "fingerprints": fingerprints}),
    );
    let names: Vec<Value> = items(input, "state_names")?
        .iter()
        .map(|c| json!({"id": c["id"], "name": state::file_name(c["id"].as_str().unwrap_or_default())}))
        .collect();
    out.insert("state_names".into(), Value::Array(names));

    let granted_cases: Vec<Vec<String>> = items(input, "granted_cases")?
        .iter()
        .map(|g| {
            g.as_array().map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
        })
        .collect();
    let mut rights = Vec::new();
    for case in items(input, "rights")? {
        let on: HashSet<String> = case["on"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default();
        let switches = |s: &str| on.contains(s);
        let mut on_sorted: Vec<&String> = on.iter().collect();
        on_sorted.sort();
        rights.push(json!({
            "on": on_sorted, "wanted": scopes::wanted_scopes(&switches), "enabled": scopes::enabled(&switches),
            "missing": granted_cases.iter().map(|g| scopes::missing_optional(&switches, g)).collect::<Vec<_>>(),
        }));
    }
    out.insert("rights".into(), Value::Array(rights));
    out.insert("holds".into(), holds(input)?);
    out.insert("schedule".into(), schedule(input)?);
    Ok(())
}

fn holds(input: &Value) -> Result<Value, String> {
    let hold_input = &input["hold_input"];
    let url = text(hold_input, "url")?;
    let refused: Vec<jsonio::Value> =
        hold_input["refused"].as_array().map(|a| a.iter().map(to_core).collect()).unwrap_or_default();
    let config = Config::from_host(&host(&[]));
    let confirmations = Confirmations::default();
    let mut state = jsonio::Map::new();
    let mut steps = Vec::new();
    for step in items(input, "holds")? {
        let now = step["now"].as_f64().unwrap_or(0.0);
        let sets: Vec<ChangeSet> = hold_input["sets"][text(step, "sets")?]
            .as_array()
            .map(|list| {
                list.iter()
                    .map(|pair| {
                        let keys = pair[1]
                            .as_array()
                            .map(|k| k.iter().filter_map(Value::as_str).map(str::to_string).collect());
                        (to_core(&pair[0]), keys.unwrap_or_default())
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut record = serde_json::Map::new();
        for key in ["op", "now", "sets", "url", "refused", "release"] {
            if !step[key].is_null() {
                record.insert(key.into(), step[key].clone());
            }
        }
        if step["op"] == "hold" {
            let confirm_url = if step["url"] == true {
                let error = ApiError::new(versekit_core::basetool::feed::MASS_CHANGE, 409)
                    .with_problem(vec![("confirmationUrl".into(), jsonio::Value::str(url))]);
                hold::confirm_address(&config, &error)
            } else {
                None
            };
            let refused_now: &[jsonio::Value] = if step["refused"] == true { &refused } else { &[] };
            hold::hold(&mut state, &confirmations, "/r", &sets, confirm_url, refused_now, now);
        } else {
            let release: HashSet<String> =
                if step["release"] == true { ["/r".to_string()].into() } else { HashSet::new() };
            let result = hold::may_send(&mut state, &confirmations, "/r", &sets, now, &release);
            record.insert("result".into(), json!(result));
        }
        let held = state.iter().find(|(k, _)| k == "held").map_or(Value::Null, |(_, v)| to_serde(v));
        let routes: serde_json::Map<String, Value> =
            hold::held_routes(&state, now).into_iter().map(|(r, s)| (r, json!(s))).collect();
        record.insert("held".into(), held);
        record.insert("routes".into(), Value::Object(routes));
        record.insert("link".into(), json!(confirmations.link("/r", now)));
        steps.push(Value::Object(record));
    }
    Ok(Value::Array(steps))
}

fn schedule(input: &Value) -> Result<Value, String> {
    let mut s = Schedule::default();
    let mut out = Vec::new();
    for record in items(input, "schedule")? {
        let step = &record["step"];
        let now = step["now"].as_f64().unwrap_or(0.0);
        match text(step, "op")? {
            "changed" => s.local_changed(now),
            "request_now" => s.request_now(),
            "disabled" => {}
            _ => {
                if s.due(now) {
                    if step["connected"] == false {
                        s.not_connected(now);
                    } else {
                        s.start();
                        if step["request_during"] == true {
                            s.request_now();
                        }
                        if step["outcome"] == "error" {
                            let error = ApiError::new(
                                step["code"].as_str().unwrap_or_default(),
                                step["status"].as_u64().unwrap_or(0) as u16,
                            );
                            s.failed(
                                now,
                                error.action(),
                                step["retry_after"].as_u64(),
                                step["jitter"].as_f64().unwrap_or(0.0),
                            );
                        } else {
                            s.succeeded(now, step["pending"] == true);
                        }
                    }
                }
            }
        }
        let next = if s.next.is_infinite() { json!("inf") } else { json!(s.next) };
        out.push(json!({"step": step, "next": next, "attempt": s.attempt, "changed_at": s.changed_at,
                        "halted": s.halted, "due": s.due(now)}));
    }
    Ok(Value::Array(out))
}

// ------------------------------------------------------------- part B

fn mode(path: &str) -> Value {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).map_or(Value::Null, |m| json!(format!("0o{:o}", m.permissions().mode() & 0o777)))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Value::Null
    }
}

fn file_secrets(folder: &str) -> Arc<SystemSecrets> {
    Arc::new(SystemSecrets::from_host(&host(&[(legacy::ENV_SECRETS, folder), (legacy::ENV_SECRETS_FILE, "1")])))
}

/// Deletes the key of the Python side when it goes out of scope, also when
/// the run breaks off.
#[cfg(windows)]
struct Foreign(String);

#[cfg(windows)]
impl Drop for Foreign {
    fn drop(&mut self) {
        if let Ok(Some(key)) = versekit_core::basetool::cng::CngKey::open(&self.0) {
            let _ = key.delete(&self.0);
        }
    }
}

/// The key of the Python side (read) and the own one (made) as public keys,
/// and the name of the own one. Elsewhere both lie in the secret folders;
/// under Windows in the CNG store under the names the Python side gives: the
/// foreign one goes after it was read, the own one is read and then deleted
/// by the Python side.
fn part_b_keys(
    input: &Value,
    python: Arc<SystemSecrets>,
    own: Arc<SystemSecrets>,
) -> Result<(Value, Value, Value), String> {
    #[cfg(windows)]
    {
        use versekit_core::basetool::cng::CngKey;
        use versekit_core::basetool::proof::DpopKey;
        let _ = (python, own);
        let names = input.get("key_names").ok_or("keine Schlüsselnamen")?;
        let theirs = text(names, "b")?;
        let _gone = Foreign(theirs.to_string());
        let read = CngKey::open(theirs).map_err(|e| e.to_string())?.ok_or("kein Python-Schlüssel")?;
        let ours = text(names, "r")?;
        let key = match CngKey::open(ours).map_err(|e| e.to_string())? {
            Some(key) => key,
            None => CngKey::create(ours).map_err(|e| e.to_string())?,
        };
        Ok((jwk_json(&read.public_jwk()), jwk_json(&key.public_jwk()), json!(ours)))
    }
    #[cfg(not(windows))]
    {
        use versekit_core::basetool::key::StoredKey;
        let _ = input;
        let read =
            StoredKey { secrets: python }.open(false).map_err(|e| e.to_string())?.ok_or("kein Python-Schlüssel")?;
        let key = StoredKey { secrets: own }.open(true).map_err(|e| e.to_string())?.ok_or("kein Schlüssel")?;
        Ok((jwk_json(&read.public_jwk()), jwk_json(&key.public_jwk()), Value::Null))
    }
}

fn part_b(input: &Value) -> Result<Value, String> {
    let python = file_secrets(text(input, "python_secrets")?);
    let read_refresh = python.load(REFRESH_SECRET).map_err(|e| e.to_string())?;
    let folder = text(input, "rust_secrets")?;
    let own = file_secrets(folder);
    own.save(REFRESH_SECRET, "R-rs-ö").map_err(|e| e.to_string())?;
    let (read_jwk, jwk, keyname) = part_b_keys(input, python, own)?;
    let file = |name: &str| Path::new(folder).join(name).to_string_lossy().into_owned();
    Ok(json!({
        "read_refresh": read_refresh, "read_jwk": read_jwk,
        "refresh": "R-rs-ö", "jwk": jwk, "keyname": keyname,
        "modes": {"folder": mode(folder), "refresh": mode(&file("refresh-token.secret")),
                  "key": mode(&file("dpop-key.secret"))},
    }))
}

// ------------------------------------------------------------- part C

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_server(input: &Value, folder: &str) -> Result<(Server, String), String> {
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let child = Command::new(text(input, "python")?)
        .args(["-I", text(input, "script")?, "--server", folder])
        .spawn()
        .map_err(|e| format!("Prüfserver: {e}"))?;
    let server = Server(child);
    let port = Path::new(folder).join("port");
    for _ in 0..200 {
        if let Ok(base) = std::fs::read_to_string(&port) {
            return Ok((server, base.trim().to_string()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("Prüfserver startet nicht".into())
}

fn server_log(folder: &str) -> Vec<Value> {
    std::fs::read_to_string(Path::new(folder).join("log.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

fn outcome(result: Result<Value, ApiError>) -> Value {
    match result {
        Ok(value) => json!({"ok": value}),
        Err(error) if error.code.starts_with("SECRET_") => json!({"secret": error.code}),
        Err(error) => json!({
            "error": error.code, "status": error.status, "action": error.action().name(),
            "retry_after": error.retry_after, "describe": error.describe(),
        }),
    }
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Idle => "idle",
        Phase::Running => "running",
        Phase::Ok => "ok",
        Phase::Error => "error",
        Phase::Stopped => "stopped",
    }
}

fn part_c(input: &Value) -> Result<Value, String> {
    let server_folder = format!("{}/server-rust", text(input, "folder")?);
    let (server, base) = start_server(input, &server_folder)?;
    let issuer = format!("{base}/realms/iri");
    let api = format!("{base}/exchange/v1");
    let net = host(&[
        (legacy::ENV_BASETOOL_ISSUER, &issuer),
        (legacy::ENV_BASETOOL_API, &api),
        (legacy::ENV_BASETOOL_CLIENT, "sandbox-client"),
    ]);
    let secrets_folder = format!("{}-protokoll", text(input, "rust_secrets")?);
    let secrets = file_secrets(&secrets_folder);
    let store: Arc<dyn SecretStore> = secrets.clone();
    let key_name = format!("VerseKit Paritaet rust {}", std::process::id());
    // Under Windows the key of the run lies in the CNG store: it goes at the
    // end, also when the run breaks off.
    #[cfg(windows)]
    let _gone = Foreign(key_name.clone());
    let vault: Arc<dyn KeyVault> = key::system_keys(store.clone(), &host(&[(legacy::ENV_BASETOOL_KEYNAME, &key_name)]));
    let transport = HttpTransport::new("", &net).map_err(|e| e.to_string())?;
    let conn = Arc::new(Connection::new(
        Config::from_host(&net),
        Arc::new(transport),
        store.clone(),
        vault,
        text(input, "version")?,
        system_clock(),
    ));
    let mut steps = Vec::new();
    let record = |steps: &mut Vec<Value>, name: &str, result: Result<Value, ApiError>| {
        steps.push(json!({
            "step": name, "result": outcome(result), "connected": conn.connected(),
            "key": matches!(store.load(KEY_SECRET), Ok(Some(_))),
            "refresh": store.load(REFRESH_SECRET).ok().flatten(),
        }));
    };
    let scopes: Vec<String> = items(input, "scopes")?.iter().filter_map(Value::as_str).map(str::to_string).collect();
    let mut device: Option<DeviceLogin> = None;
    let started = conn.start_login(&scopes).map(|d| {
        let shown = json!({"user_code": d.user_code, "uri": d.verification_uri.replace(&base, "<base>"),
                           "interval": d.interval, "scopes": d.scopes});
        device = Some(d);
        shown
    });
    record(&mut steps, "start_login", started);
    for _ in 0..3 {
        let polled = match device.as_mut() {
            Some(d) => conn
                .poll_login(d)
                .map(|p| json!({"result": p.name(), "interval": d.interval, "granted": conn.granted()})),
            None => Err(ApiError::new("NO_LOGIN", 0)),
        };
        record(&mut steps, "poll", polled);
    }
    let answer = |r: Result<jsonio::Map, ApiError>| r.map(|m| map_to_serde(&m));
    record(&mut steps, "service_document", answer(conn.service_document()));
    record(&mut steps, "org_stock", answer(conn.request("GET", "/org-stock", None, &[("limit", "500")], None)));
    let body =
        jsonio::loads(r#"{"ops": [{"op": "note", "unit": "u1", "note": "Grüße"}]}"#).map_err(|e| e.to_string())?;
    record(
        &mut steps,
        "mass_change",
        answer(conn.request("POST", "/org-stock/changes", Some(&body), &[], Some(&new_idempotency_key()))),
    );
    record(&mut steps, "http_503", answer(conn.request("GET", "/me/kaputt", None, &[], None)));
    record(&mut steps, "rate_limited", answer(conn.request("GET", "/me/limit", None, &[("cursor", "geheim")], None)));
    let account = text(input, "account")?;
    record(&mut steps, "account_check", conn.account_check(account).map(Value::String));
    record(&mut steps, "label", answer(conn.label_installation("VerseKit Parität")));
    record(&mut steps, "dead_refresh", answer(conn.request("GET", "/me/dead", None, &[], None)));
    store.save(REFRESH_SECRET, "R-neu").map_err(|e| e.to_string())?;
    record(&mut steps, "revoked", answer(conn.request("GET", "/me/revoked", None, &[], None)));
    store.save(REFRESH_SECRET, "R-letzt").map_err(|e| e.to_string())?;
    conn.disconnect();
    record(&mut steps, "disconnect", Ok(Value::Null));

    let mut home = Host::current();
    home.set_var(legacy::ENV_HOME, text(input, "rust_home")?);
    let paths = Arc::new(Paths::new(home));
    paths.set_setting(scopes::SETTING_ORGSTOCK, jsonio::Value::Bool(true));
    paths.set_setting(scopes::SETTING_LABEL, jsonio::Value::str("Parität-PC"));
    let own = account.to_string();
    let handle = SyncHandle::without_worker(Setup {
        conn: conn.clone(),
        paths,
        clock: system_clock(),
        version: text(input, "version")?.into(),
        own_account: Arc::new(move || Some(own.clone())),
        waker: Arc::new(|| {}),
        areas: Vec::new(),
    });
    let pass_start = server_log(&server_folder).len();
    let mut passes = Vec::new();
    for refresh in ["R-pass", "R-pass2"] {
        store.save(REFRESH_SECRET, refresh).map_err(|e| e.to_string())?;
        let result = outcome(handle.pass_now().map(|()| Value::Null));
        let status = handle.status().get().1.map(|s| (*s).clone()).unwrap_or_default();
        passes.push(json!({
            "result": result, "state": phase_name(status.phase), "code": status.code,
            "installation_id": status.installation_id, "account": status.account,
        }));
    }
    drop(server);
    Ok(json!({"steps": steps, "passes": passes, "pass_start": pass_start, "log": server_log(&server_folder)}))
}

/// With `counter_check` one value of each part is changed, so the comparison
/// must report all four.
pub fn run(python_json: &str, out: &str, counter_check: bool) -> Result<(), String> {
    let input: Value =
        serde_json::from_slice(&std::fs::read(python_json).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let core_input = jsonio::read_json(python_json).map_err(|e| e.to_string())?;
    let mut result = serde_json::Map::new();
    part_a(&input, &core_input, &mut result)?;
    result.insert("secrets".into(), part_b(&input)?);
    result.insert("protocol".into(), part_c(&input)?);
    if counter_check {
        result["htu"][0]["result"] = json!({"ok": "gegenprobe"});
        result["schedule"][4]["attempt"] = json!(99);
        result["secrets"]["read_refresh"] = json!("gegenprobe");
        result["protocol"]["steps"][0]["result"] = json!({});
    }
    std::fs::write(out, serde_json::to_vec_pretty(&Value::Object(result)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
