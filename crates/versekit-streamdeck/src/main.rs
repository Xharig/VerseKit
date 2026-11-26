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

//! The Stream Deck plugin of VerseKit. The Stream Deck program starts it with
//! `-port <n> -pluginUUID <id> -registerEvent <name> -info <json>`; `--paket <folder>` writes the plugin folder and
//! the installable file instead.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod game_style;
mod pack;
mod paint;
mod plugin;
mod render;
mod verse;
mod ws;

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use versekit_core::paths::{Host, Paths};

use crate::ws::{DeckLink, WsClient};

/// How often VerseKit is asked for its state.
const POLL: Duration = Duration::from_secs(1);

/// The arguments the Stream Deck program passes.
#[derive(Debug, PartialEq, Eq)]
struct Launch {
    port: u16,
    uuid: String,
    register: String,
    info: String,
}

fn launch_args(args: &[String]) -> Option<Launch> {
    let value = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    Some(Launch {
        port: value("-port")?.parse().ok()?,
        uuid: value("-pluginUUID")?,
        register: value("-registerEvent")?,
        info: value("-info").unwrap_or_default(),
    })
}

/// The devices of the `-info` argument as `deviceDidConnect` events.
fn devices_of(info: &str) -> Vec<Value> {
    let Ok(value) = serde_json::from_str::<Value>(info) else { return Vec::new() };
    value["devices"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|d| json!({"event": "deviceDidConnect", "device": d["id"], "deviceInfo": {"type": d["type"], "name": d["name"]}}))
                .collect()
        })
        .unwrap_or_default()
}

/// The folder of the profiles beside the program.
fn profiles_folder() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok().and_then(|exe| exe.parent().map(|p| p.join("profiles")))
}

/// The profiles that lie beside the program (`profiles/<name>.streamDeckProfile`).
fn shipped_profiles() -> Vec<String> {
    let Some(folder) = profiles_folder() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| e.file_name().to_str()?.strip_suffix(".streamDeckProfile").map(String::from))
        .collect()
}

/// What each shipped profile holds, read once: the names of its pages in their order, and the game actions on its
/// keys and dials (all pages); a file that cannot be read has neither.
type ShippedContent = (std::collections::HashMap<String, Vec<String>>, std::collections::HashMap<String, Vec<String>>);

fn shipped_content(profiles: &[String]) -> ShippedContent {
    let Some(folder) = profiles_folder() else { return Default::default() };
    let mut pages = std::collections::HashMap::new();
    let mut actions = std::collections::HashMap::new();
    for name in profiles {
        let Ok(bytes) = std::fs::read(folder.join(format!("{name}.streamDeckProfile"))) else { continue };
        pages.insert(name.clone(), plugin::profile_pages(&bytes));
        actions.insert(name.clone(), plugin::profile_game_actions(&bytes));
    }
    (pages, actions)
}

enum Incoming {
    Deck(Value),
    Closed,
}

fn run(launch: &Launch) -> std::io::Result<()> {
    let mut link = WsClient::connect(launch.port)?;
    link.send_text(&json!({"event": launch.register, "uuid": launch.uuid}).to_string())?;
    let mut reader = link.try_clone()?;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new().name("deck-lesen".into()).spawn(move || {
        loop {
            match reader.recv_text() {
                Ok(Some(text)) => {
                    if let Ok(value) = serde_json::from_str::<Value>(&text)
                        && tx.send(Incoming::Deck(value)).is_err()
                    {
                        return;
                    }
                }
                Ok(None) | Err(_) => {
                    let _ = tx.send(Incoming::Closed);
                    return;
                }
            }
        }
    })?;
    let mut verse = verse::Http::new(Paths::new(Host::current()));
    let mut plugin = plugin::Plugin::new(&launch.uuid, shipped_profiles());
    (plugin.pages, plugin.profile_actions) = shipped_content(&plugin.profiles);
    let send = |link: &mut WsClient, out: Vec<Value>| -> std::io::Result<()> {
        for message in out {
            link.send_text(&message.to_string())?;
        }
        Ok(())
    };
    send(&mut link, plugin.start())?;
    let out = plugin.poll(&mut verse);
    send(&mut link, out)?;
    for device in devices_of(&launch.info) {
        let out = plugin.on_event(&device, &mut verse);
        send(&mut link, out)?;
    }
    let mut last_poll = Instant::now();
    loop {
        // While a dial runs through its pictures, the loop wakes for each of them.
        let mut wait = POLL.saturating_sub(last_poll.elapsed());
        if plugin.busy() {
            wait = wait.min(plugin::DIAL_FRAME / 2);
        }
        match rx.recv_timeout(wait) {
            Ok(Incoming::Deck(message)) => {
                let out = plugin.on_event(&message, &mut verse);
                send(&mut link, out)?;
            }
            Ok(Incoming::Closed) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if plugin.busy() {
            let out = plugin.tick(&mut verse);
            send(&mut link, out)?;
        }
        if last_poll.elapsed() >= POLL {
            last_poll = Instant::now();
            let out = plugin.poll(&mut verse);
            send(&mut link, out)?;
        }
    }
    plugin.release_all(&mut verse);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--paket") {
        let Some(target) = args.get(i + 1) else {
            eprintln!("--paket <Ordner>");
            std::process::exit(2);
        };
        match pack::write(std::path::Path::new(target)) {
            Ok(file) => println!("{}", file.display()),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let Some(launch) = launch_args(&args) else {
        eprintln!("Dieses Programm startet das Stream-Deck-Programm. / The Stream Deck program starts this program.");
        std::process::exit(2);
    };
    if run(&launch).is_err() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arguments_of_the_stream_deck_program_are_read() {
        let args: Vec<String> =
            ["x", "-port", "28196", "-pluginUUID", "ABC", "-registerEvent", "registerPlugin", "-info", "{}"]
                .iter()
                .map(|s| s.to_string())
                .collect();
        assert_eq!(
            launch_args(&args),
            Some(Launch { port: 28196, uuid: "ABC".into(), register: "registerPlugin".into(), info: "{}".into() })
        );
        assert_eq!(launch_args(&args[..3]), None);
    }

    #[test]
    fn the_devices_of_the_start_become_connect_events() {
        let info = r#"{"devices":[{"id":"D1","name":"iPad","type":3,"size":{"columns":8,"rows":4}}]}"#;
        let events = devices_of(info);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["deviceInfo"]["type"], 3);
        assert!(devices_of("kein json").is_empty());
    }
}
