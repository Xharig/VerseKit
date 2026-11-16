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

//! The template server against its description `tools/orga-standard/v1/openapi.yaml`: every path and method
//! there is answered (not `404`, not `405`), the rights and the discovery document match, and the example
//! configuration starts.

use std::path::PathBuf;

use serde_json::json;
use versekit_orgserver::http::Request;
use versekit_orgserver::{Config, SCOPES, Server};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn description() -> String {
    std::fs::read_to_string(root().join("tools").join("orga-standard").join("v1").join("openapi.yaml")).unwrap()
}

/// The `(METHOD, path)` pairs of the description: a path at two spaces, its methods at four.
fn operations(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut in_paths = false;
    let mut path = None;
    for line in text.lines() {
        if !line.starts_with(' ') {
            in_paths = line == "paths:";
            continue;
        }
        if !in_paths {
            continue;
        }
        if let Some(name) = line.strip_prefix("  /").and_then(|l| l.strip_suffix(':')) {
            path = Some(format!("/{name}"));
        } else if let (Some(p), Some(method)) = (&path, line.strip_prefix("    ").and_then(|l| l.strip_suffix(':')))
            && ["get", "post", "put", "delete", "patch"].contains(&method)
        {
            found.push((method.to_uppercase(), p.clone()));
        }
    }
    found
}

fn server() -> Server {
    Server::new(
        Config::new("https://orga.example.org", "Orga", "ORG")
            .with_member("m1", "Pilot", "0123456789abcdef")
            .with_order(json!({ "id": "a1", "title": "Erz", "items": [{ "item": "Quantanium", "quantity": 1 }] })),
    )
}

#[test]
fn every_operation_of_the_description_is_answered() {
    let operations = operations(&description());
    assert_eq!(operations.len(), 12, "{operations:?}");
    let server = server();
    for (method, path) in &operations {
        let target = path.replace("{orderId}", "a1");
        let answer = server.handle(&Request::new(method, &target, &[], b""));
        assert!(![404, 405].contains(&answer.status), "{method} {path}: {}", answer.status);
        if path.starts_with("/versekit/v1/") && !path.contains("/device/") && !path.ends_with("/token") && !path.ends_with("/revoke") {
            assert_eq!(answer.status, 401, "{method} {path} without a token");
        }
    }
    // The counter-check: a path the description does not name is unknown to the server.
    assert_eq!(server.handle(&Request::new("GET", "/versekit/v1/ships", &[], b"")).status, 404);
}

#[test]
fn the_rights_and_the_discovery_document_match_the_description() {
    let text = description();
    let listed = text.lines().find(|l| l.trim_start().starts_with("enum: [stock:read")).unwrap();
    for scope in SCOPES {
        assert!(listed.contains(scope), "{scope}");
    }
    let document = server().handle(&Request::new("GET", "/.well-known/versekit-org.json", &[], b"")).json_body();
    let entry = &document["versions"]["1"];
    for key in ["api_base", "device_authorization_endpoint", "token_endpoint", "revocation_endpoint", "scopes_supported"] {
        assert!(!entry[key].is_null(), "{key}");
        assert!(text.contains(&format!("        {key}:")), "{key} is not in the description");
    }
    assert_eq!(document["standard"], "versekit-org");
}

#[test]
fn the_example_configuration_starts() {
    let text = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.example.json")).unwrap();
    let why = Config::from_json(&text).unwrap_err();
    assert!(why.contains("m1") && why.contains("shorter than 16"), "unchanged it must not start: {why}");
    let filled = text.replacen("\"key\": \"\"", "\"key\": \"eigener-zufaelliger-schluessel-1\"", 1).replacen(
        "\"key\": \"\"",
        "\"key\": \"eigener-zufaelliger-schluessel-2\"",
        1,
    );
    let config = Config::from_json(&filled).unwrap();
    assert_eq!((config.members.len(), config.orders.len()), (2, 1));
    let old = filled.replace("eigener-zufaelliger-schluessel-1", "mindestens-16-zeichen-zufall-1");
    assert!(Config::from_json(&old).unwrap_err().contains("example key"), "the key of an older template is refused");
}
