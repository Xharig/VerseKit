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

//! Starts the template server: `versekit-orgserver [--config FILE] [--state FILE] [--listen ADDRESS]`, or the
//! same through `VERSEKIT_ORG_CONFIG`, `VERSEKIT_ORG_STATE`, `VERSEKIT_ORG_LISTEN`.
//!
//! `--admin-password` shows the generated admin password in the terminal (never in the log), `--healthcheck` asks
//! `/healthz` of the running server for the health check of the container.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use versekit_orgserver::{Config, Server, http, net, setup, updates};

fn setting(args: &[String], flag: &str, variable: &str, default: &str) -> String {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
        .or_else(|| std::env::var(variable).ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| default.to_string())
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2);
}

fn data_dir(state_file: &str) -> PathBuf {
    Path::new(state_file).parent().filter(|d| !d.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

/// Asks `/healthz` on the own computer; 0 when it answers 200.
fn healthcheck(listen: &str) -> i32 {
    let port = listen.rsplit(':').next().unwrap_or("8080");
    let Ok(mut stream) = TcpStream::connect(format!("127.0.0.1:{port}")) else { return 1 };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(4)));
    if stream.write_all(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").is_err() {
        return 1;
    }
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    if answer.starts_with("HTTP/1.1 200") { 0 } else { 1 }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "versekit-orgserver {} [--config FILE] [--state FILE] [--listen ADDRESS] | --admin-password | --healthcheck",
            env!("CARGO_PKG_VERSION")
        );
        return;
    }
    let config_file = setting(&args, "--config", "VERSEKIT_ORG_CONFIG", "config.json");
    let state_file = setting(&args, "--state", "VERSEKIT_ORG_STATE", "state.json");
    let listen = setting(&args, "--listen", "VERSEKIT_ORG_LISTEN", "127.0.0.1:8080");
    if args.iter().any(|a| a == "--healthcheck") {
        std::process::exit(healthcheck(&listen));
    }
    let dir = data_dir(&state_file);
    if args.iter().any(|a| a == "--admin-password") {
        let set = std::env::var("VERSEKIT_ORG_ADMIN_PASSWORD").is_ok_and(|v| !v.trim().is_empty())
            || std::env::var("VERSEKIT_ORG_ADMIN_PASSWORD_FILE").is_ok_and(|v| !v.trim().is_empty());
        if set {
            println!("The admin password is set through the environment (VERSEKIT_ORG_ADMIN_PASSWORD or _FILE).");
            return;
        }
        match std::fs::read_to_string(dir.join(setup::PASSWORD_FILE)) {
            Ok(text) => println!("{}", text.trim()),
            Err(_) => fail("No admin password yet: start the server once, it creates one."),
        }
        return;
    }

    let text = std::fs::read_to_string(&config_file).unwrap_or_else(|error| fail(&format!("cannot read {config_file}: {error}")));
    let mut config = Config::from_json(&text)
        .and_then(|c| c.with_env(|name| std::env::var(name).ok()))
        .unwrap_or_else(|error| fail(&format!("{config_file}: {error}")));
    config.state_file = Some(PathBuf::from(&state_file));
    match &config.admin_source {
        setup::AdminSource::Off => {
            eprintln!("WARNING: the administration /admin is turned off (VERSEKIT_ORG_ADMIN=off)");
        }
        setup::AdminSource::Environment => println!("admin password: set through the environment"),
        _ => {
            let (password, created) = setup::admin_password(&dir).unwrap_or_else(|error| fail(&error));
            let file = dir.join(setup::PASSWORD_FILE);
            if created {
                println!(
                    "admin password created in {} (readable for the owner only); show it with: versekit-orgserver --admin-password",
                    file.display()
                );
            } else {
                println!("admin password: from {}", file.display());
            }
            config = config.with_generated_password(&password, file).unwrap_or_else(|error| fail(&error));
        }
    }
    if config.test_mode {
        eprintln!("WARNING: TEST MODE without HTTPS (VERSEKIT_ORG_TESTBETRIEB=1): for trying out only, never for real members");
    }
    let listener = TcpListener::bind(&listen).unwrap_or_else(|error| fail(&format!("cannot listen on {listen}: {error}")));
    let address = config.public_url.clone();
    let checking = config.update_check;
    let server = Arc::new(Server::open(config).unwrap_or_else(|error| fail(&error)));
    for note in server.notes() {
        println!("{note}");
    }
    for finding in setup::findings(&server, versekit_orgserver::unix_now()).iter().filter(|f| !f.ok) {
        eprintln!("self-check: {}", finding.en);
    }
    if checking {
        let watched = server.clone();
        std::thread::spawn(move || {
            let client = net::Client::new();
            std::thread::sleep(Duration::from_secs(60));
            loop {
                let mut found = updates::UpdateState { checked: versekit_orgserver::unix_now(), ..updates::UpdateState::default() };
                match updates::check(&client, env!("CARGO_PKG_VERSION")) {
                    Ok(offer) => found.offer = offer,
                    Err(why) => found.failed = Some(why),
                }
                if let Some(offer) = &found.offer {
                    println!("update available: versekit-orgserver {} ({})", offer.version, offer.url);
                }
                watched.set_update(found);
                std::thread::sleep(Duration::from_secs(updates::INTERVAL));
            }
        });
    }
    let local = listener.local_addr().map_or_else(|_| listen.clone(), |a| a.to_string());
    println!("versekit-orgserver {} (versekit-org v1) for {address}, listening on {local}", env!("CARGO_PKG_VERSION"));
    let _ = std::io::stdout().flush();
    http::serve(listener, Arc::new(move |request: &http::Request| server.handle(request)));
}
