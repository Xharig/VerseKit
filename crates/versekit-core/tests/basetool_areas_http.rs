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

//! A pass of the blueprint and stock areas through the real transport and
//! connection, against the stand-in of the Basetool as a server on
//! `127.0.0.1`: the requests carry the token and a proof, the pass ends as it
//! does in memory. Nothing leaves the computer; the real Basetool is never asked.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use versekit_core::basetool::blueprint_area::{BlueprintArea, FileBlueprintHost};
use versekit_core::basetool::config::Config;
use versekit_core::basetool::connection::{Connection, system_clock};
use versekit_core::basetool::exchange_double::ExchangeDouble;
use versekit_core::basetool::http::HttpTransport;
use versekit_core::basetool::key::StoredKey;
use versekit_core::basetool::secrets::{MemorySecrets, REFRESH_SECRET, SecretStore};
use versekit_core::basetool::stock_area::StockArea;
use versekit_core::basetool::sync::{Setup, SyncHandle};
use versekit_core::collection::{self, Collection, NoCatalog, NoExports};
use versekit_core::jsonio::Value;
use versekit_core::legacy;
use versekit_core::paths::{Host, Paths};
use versekit_core::phrases::{BackRoute, NoArchive};
use versekit_core::pypath::Flavor;

/// What the server saw of one request.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    headers: HashMap<String, String>,
}

/// The stand-in as a server on 127.0.0.1; the base address and the log.
fn serve(double: Arc<ExchangeDouble>) -> (String, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("port");
    let base = format!("http://127.0.0.1:{}", listener.local_addr().expect("address").port());
    let log = Arc::new(Mutex::new(Vec::new()));
    let (seen, root) = (log.clone(), base.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() || line.is_empty() {
                continue;
            }
            let mut words = line.split_whitespace();
            let method = words.next().unwrap_or_default().to_string();
            let target = words.next().unwrap_or_default().to_string();
            let mut headers = HashMap::new();
            loop {
                let mut h = String::new();
                if reader.read_line(&mut h).is_err() || h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    headers.insert(k.trim().to_lowercase(), v.trim().to_string());
                }
            }
            let length: usize = headers.get("content-length").and_then(|l| l.parse().ok()).unwrap_or(0);
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
            seen.lock().expect("log").push(Seen { method: method.clone(), path: target.clone(), headers });
            let issuer = format!("{root}/realms/iri");
            let api = format!("{root}/exchange/v1");
            let answer = double.respond(&method, &format!("{root}{target}"), Some(&body), &issuer, &api);
            let mut text = format!(
                "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                answer.status,
                answer.body.len()
            );
            for (k, v) in &answer.headers {
                text.push_str(&format!("{k}: {v}\r\n"));
            }
            text.push_str("\r\n");
            let _ = stream.write_all(text.as_bytes());
            let _ = stream.write_all(&answer.body);
        }
    });
    (base, log)
}

#[test]
fn a_pass_through_the_real_transport_carries_token_and_proof_and_ends_as_in_memory() {
    let double = Arc::new(ExchangeDouble::new());
    let (base, log) = serve(double.clone());
    let dir = std::env::temp_dir().join(format!("versekit-areas-http-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    let data = dir.to_string_lossy().replace('\\', "/");
    let issuer = format!("{base}/realms/iri");
    let api = format!("{base}/exchange/v1");
    // An empty game folder of its own: the logs of the machine must not be read by a test.
    let game = format!("{data}/Spiel");
    std::fs::create_dir_all(&game).expect("game folder");
    let host = Host::new(
        Flavor::native(),
        &[
            (legacy::ENV_HOME, &data),
            (versekit_core::paths::ENV_INSTALL_DIR, &game),
            (legacy::ENV_BASETOOL_ISSUER, &issuer),
            (legacy::ENV_BASETOOL_API, &api),
            (legacy::ENV_BASETOOL_CLIENT, "sandbox-client"),
        ],
        &data,
    );
    let paths = Arc::new(Paths::new(host.clone()));
    for switch in ["basetool_bauplaene", "basetool_lager"] {
        assert!(paths.set_setting(switch, Value::Bool(true)));
    }
    let secrets = Arc::new(MemorySecrets::default());
    secrets.save(REFRESH_SECRET, "R1").expect("secret");
    let keys = Arc::new(StoredKey { secrets: secrets.clone() });
    let conn = Connection::new(
        Config::from_host(&host),
        Arc::new(HttpTransport::new("", &host).expect("transport")),
        secrets,
        keys,
        "5.0.0",
        system_clock(),
    );
    let sync = SyncHandle::without_worker(Setup {
        conn: Arc::new(conn),
        paths: paths.clone(),
        clock: system_clock(),
        version: "5.0.0".into(),
        own_account: Arc::new(|| Some("PlayerOne".to_string())),
        waker: Arc::new(|| {}),
        areas: vec![
            Box::new(BlueprintArea::new(Arc::new(FileBlueprintHost::new(paths.clone(), Arc::new(NoCatalog))))),
            Box::new(StockArea),
        ],
    });
    // One blueprint here, one over there.
    let names = BackRoute { paths: &paths, archive: &NoArchive };
    let collection = Collection {
        paths: &paths,
        catalog: &NoCatalog,
        names: &names,
        exports: &NoExports,
        clock: collection::now_text,
    };
    let mut stock = collection.load();
    collection.add(&mut stock, "Aether Rifle", "hand", None);
    assert!(collection.save(&mut stock));
    double.add_blueprint("Corsair Legs", false, Some("2026-10-20T08:30:00Z"));

    sync.pass_now().expect("a pass over the real transport");

    assert_eq!(double.blueprint_names(), ["Aether Rifle", "Corsair Legs"]);
    let names = collection::names(&collection.load());
    assert!(names.iter().any(|n| n == "Corsair Legs"), "{names:?}");
    // Every request of the API carried the token and a proof; the proof is for this very server.
    let seen = log.lock().expect("log").clone();
    let api_requests: Vec<&Seen> = seen.iter().filter(|s| s.path.starts_with("/exchange/v1")).collect();
    assert!(api_requests.len() >= 4, "{seen:?}");
    for request in &api_requests {
        assert!(request.headers.get("authorization").is_some_and(|a| a.starts_with("DPoP ")), "{request:?}");
        assert!(request.headers.get("dpop").is_some_and(|p| p.split('.').count() == 3), "{request:?}");
        assert!(request.headers.get("host").is_some_and(|h| h.starts_with("127.0.0.1")), "{request:?}");
    }
    let sends: Vec<&&Seen> =
        api_requests.iter().filter(|s| s.method == "POST" && s.path.ends_with("/changes")).collect();
    assert_eq!(sends.len(), 1, "one send for the blueprint that was only here");
    assert!(sends[0].headers.contains_key("idempotency-key"), "{:?}", sends[0]);
    let _ = std::fs::remove_dir_all(dir);
}
