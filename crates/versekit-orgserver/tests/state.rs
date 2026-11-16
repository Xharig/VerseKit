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

//! The state file across updates: a newer server moves older data to its format after a backup, and the logins of
//! the members stay; an older server refuses newer data with a clear reason and touches nothing; a broken file is
//! never replaced by an empty state.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use versekit_orgserver::http::Request;
use versekit_orgserver::{Config, STATE_FORMAT, Server, hash};

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("versekit-orgserver-state-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn config(dir: &Path) -> Config {
    let mut config = Config::new("https://orga.example.org", "Prüforga", "PRF").with_member("vega", "Vega", "schluessel-von-vega-0123");
    config.state_file = Some(dir.join("state.json"));
    config
}

/// A state file as version 1.0.0 before the move wrote it: format 1, a login of Vega, her stock and one order.
fn format_one() -> Value {
    let mut refresh = serde_json::Map::new();
    refresh.insert(
        hash("vkr_alter_token_von_vega"),
        json!({ "member": "vega", "scopes": ["stock:read"], "expires": 0, "family": "f1" }),
    );
    json!({
        "format": 1,
        "access": {},
        "refresh": refresh,
        "stock": { "vega": [{ "item": "Quantanium", "quantity": 4.0 }] },
        "blueprints": {},
        "orders": [{ "id": "a1", "title": "Erz", "status": "open", "items": [{ "item": "Quantanium", "quantity": 4.0, "delivered": 0.0 }] }],
        "deliveries": {}
    })
}

fn backups(dir: &Path) -> Vec<PathBuf> {
    let mut list: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("backup-")))
        .collect();
    list.sort();
    list
}

#[test]
fn older_data_move_after_a_backup_and_the_logins_stay() {
    let dir = folder("move");
    let old = format_one().to_string();
    std::fs::write(dir.join("state.json"), &old).unwrap();
    let server = Server::open(config(&dir)).expect("the older format is moved");
    let notes = server.notes().join("\n");
    assert!(notes.contains("format 1") && notes.contains(&format!("format {STATE_FORMAT}")) && notes.contains("backup-"), "{notes}");

    let saved: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("state.json")).unwrap()).unwrap();
    assert_eq!(saved["format"], STATE_FORMAT, "the file is written in the new format at once");
    assert_eq!(saved["orders"][0]["title"], "Erz");
    let kept = backups(&dir);
    assert_eq!(kept.len(), 1, "one backup");
    assert_eq!(std::fs::read_to_string(kept[0].join("state.json")).unwrap(), old, "the backup holds the old file as it was");

    let form = "grant_type=refresh_token&refresh_token=vkr_alter_token_von_vega";
    let answer = server.handle(&Request::new("POST", "/versekit/v1/token", &[], form.as_bytes()));
    assert_eq!(answer.status, 200, "Vega stays logged in across the update: {}", String::from_utf8_lossy(&answer.body));
    assert_eq!(server.stock_of("vega").len(), 1);

    drop(server);
    let again = Server::open(config(&dir)).unwrap();
    assert!(again.notes().iter().all(|n| !n.contains("backup-")), "the current format needs no backup");
    assert_eq!(backups(&dir).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn newer_data_stop_an_older_server_with_the_reason() {
    let dir = folder("newer");
    let newer = json!({ "format": STATE_FORMAT + 1, "orders": [] }).to_string();
    std::fs::write(dir.join("state.json"), &newer).unwrap();
    let Err(why) = Server::open(config(&dir)) else { panic!("an older server must not start on newer data") };
    assert!(why.contains(&format!("format {}", STATE_FORMAT + 1)) && why.contains(&format!("up to {STATE_FORMAT}")), "{why}");
    assert!(why.contains("newer image") && why.contains("backup"), "the reason says what to do: {why}");
    assert_eq!(std::fs::read_to_string(dir.join("state.json")).unwrap(), newer, "the newer data stay untouched");
    assert!(backups(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_broken_state_file_is_never_replaced_by_an_empty_state() {
    let dir = folder("broken");
    std::fs::write(dir.join("state.json"), "{ kaputt").unwrap();
    let Err(why) = Server::open(config(&dir)) else { panic!("a broken file must stop the start") };
    assert!(why.contains("not valid JSON"), "{why}");
    assert_eq!(std::fs::read_to_string(dir.join("state.json")).unwrap(), "{ kaputt");
    std::fs::write(dir.join("state.json"), r#"{"orders": []}"#).unwrap();
    assert!(Server::open(config(&dir)).is_err(), "a file without a format number");
    std::fs::remove_file(dir.join("state.json")).unwrap();
    let fresh = Server::open(config(&dir)).expect("no file yet is a first start");
    assert!(fresh.notes().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
