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

//! The conformance checks of `versekit-orgcheck`: the template server passes every one, and a server that answers
//! wrong on purpose in one place turns exactly the check of that place red.

use serde_json::json;
use versekit_orgserver::conformance::{Exchange, Options, Outcome, Prompt, Report, run};
use versekit_orgserver::http::{Request, Response};
use versekit_orgserver::{Config, Server};

const BASE: &str = "https://orga.example.org";
const KEY: &str = "schluessel-des-testpiloten";

/// What a broken server does instead: it gets the method, the address, the body sent and the way to the template
/// server, which it may take or not, and gives the answer.
type Tamper = fn(&str, &str, &[u8], &dyn Fn() -> Response) -> Option<Response>;

/// The template server in this process; `tamper` may answer differently or not at all.
struct Local<'a> {
    server: &'a Server,
    tamper: Tamper,
}

impl Exchange for Local<'_> {
    fn exchange(&self, method: &str, url: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<Response, String> {
        let target = url.strip_prefix(BASE).ok_or_else(|| format!("left the server: {url}"))?;
        let target = if target.is_empty() { "/" } else { target };
        let forward = || self.server.handle(&Request::new(method, target, headers, body));
        (self.tamper)(method, url, body, &forward).ok_or_else(|| "no answer".to_string())
    }
}

fn honest(_: &str, _: &str, _: &[u8], forward: &dyn Fn() -> Response) -> Option<Response> {
    Some(forward())
}

fn server() -> Server {
    let mut config = Config::new(BASE, "Prüforga", "PRF")
        .with_member("tester", "Testpilot", KEY)
        .with_order(json!({"id": "a1", "title": "Erz", "items": [{"item": "Quantanium", "quantity": 4, "unit": "scu"}]}));
    config.poll_interval = 0;
    config.rate_limit = 30;
    Server::new(config)
}

fn check(tamper: Tamper) -> Report {
    let server = server();
    let exchange = Local { server: &server, tamper };
    let mut confirm = |prompt: &Prompt| assert!(server.approve(&prompt.user_code, "tester", None), "the code {prompt:?}");
    let options = Options { write: true, burst: 60, german: false, max_wait: 5 };
    run(BASE, &exchange, &mut confirm, &|_| {}, &options)
}

fn json_answer(status: u16, value: serde_json::Value) -> Response {
    Response::json(status, &value)
}

#[test]
fn the_template_server_passes_every_check() {
    let report = check(honest);
    assert!(report.passed(), "{}", report.render(false));
    for id in [
        "discovery",
        "device_invalid_scope",
        "device_code",
        "token_pending",
        "token_unknown_code",
        "token_unsupported_grant",
        "tokens",
        "me",
        "scope_limit",
        "unauthorized",
        "method_not_allowed",
        "tokens_full",
        "stock_read",
        "blueprints_read",
        "orders_read",
        "stock_write",
        "blueprints_write",
        "deliveries_errors",
        "refresh_rotation",
        "rate_limit",
        "revoke",
    ] {
        assert_eq!(report.outcome(id), Some(&Outcome::Green), "{id}\n{}", report.render(false));
    }
    assert!(matches!(report.outcome("maintenance"), Some(Outcome::Skipped(_))));
    let german = report.render(true);
    assert!(german.contains("grün") && german.contains("0 rot"), "{german}");
}

#[test]
fn a_server_that_offers_only_the_stock_passes_and_new_rights_are_passed_over() {
    // An org with stock per place, member, quality and quantity, without blueprints and orders, and one right of a
    // later version of the standard: nothing is red, what is not offered is skipped.
    let report = check(|_, url, _, forward| {
        let answer = forward();
        Some(if url.ends_with("/.well-known/versekit-org.json") {
            let mut value = answer.json_body();
            value["versions"]["1"]["scopes_supported"] = json!(["stock:read", "stock:write", "maps:read"]);
            json_answer(200, value)
        } else {
            answer
        })
    });
    assert!(report.passed(), "{}", report.render(false));
    for id in ["discovery", "tokens", "tokens_full", "stock_read", "stock_write", "refresh_rotation", "rate_limit", "revoke"] {
        assert_eq!(report.outcome(id), Some(&Outcome::Green), "{id}\n{}", report.render(false));
    }
    for id in ["blueprints_read", "orders_read", "blueprints_write", "deliveries_errors", "scope_limit"] {
        assert!(matches!(report.outcome(id), Some(Outcome::Skipped(_))), "{id}\n{}", report.render(false));
    }
}

#[test]
fn a_server_that_lets_anyone_in_is_red() {
    let report = check(|_, _, _, forward| {
        let answer = forward();
        Some(if answer.status == 401 { json_answer(200, json!({"member": {"id": "x", "name": "x"}, "scopes": []})) } else { answer })
    });
    assert!(matches!(report.outcome("unauthorized"), Some(Outcome::Red(_))), "{}", report.render(false));
    assert!(!report.passed());
}

#[test]
fn a_refresh_without_rotation_is_red() {
    let report = check(|_, url, body, forward| {
        let mut answer = forward();
        let sent = String::from_utf8_lossy(body).to_string();
        if url.ends_with("/token") && sent.contains("grant_type=refresh_token") && answer.status == 200 {
            let old = sent.split('&').find_map(|p| p.strip_prefix("refresh_token=")).unwrap_or_default().to_string();
            let mut value = answer.json_body();
            value["refresh_token"] = json!(old);
            answer.body = value.to_string().into_bytes();
        }
        Some(answer)
    });
    assert!(matches!(report.outcome("refresh_rotation"), Some(Outcome::Red(why)) if why.contains("rotate")), "{}", report.render(false));
}

#[test]
fn a_revocation_that_does_nothing_is_red() {
    let report = check(|_, url, _, forward| Some(if url.ends_with("/revoke") { json_answer(200, json!({})) } else { forward() }));
    assert!(matches!(report.outcome("revoke"), Some(Outcome::Red(why)) if why.contains("still works")), "{}", report.render(false));
}

#[test]
fn a_server_without_a_rate_limit_is_red() {
    let report = check(|_, _, _, forward| {
        let answer = forward();
        Some(if answer.status == 429 { json_answer(200, json!({})) } else { answer })
    });
    assert!(matches!(report.outcome("rate_limit"), Some(Outcome::Red(why)) if why.contains("no 429")), "{}", report.render(false));
}

#[test]
fn a_wrong_error_code_for_a_missing_right_is_red() {
    let report = check(|_, _, _, forward| {
        let answer = forward();
        Some(if answer.status == 403 {
            let mut changed = json_answer(403, json!({"status": 403, "code": "FORBIDDEN"}));
            changed.headers[0].1 = "application/problem+json".into();
            changed
        } else {
            answer
        })
    });
    assert!(matches!(report.outcome("scope_limit"), Some(Outcome::Red(why)) if why.contains("FORBIDDEN")), "{}", report.render(false));
}

#[test]
fn a_made_up_cursor_that_is_taken_is_red() {
    let report = check(|_, url, _, forward| {
        Some(if url.contains("cursor=versekit-orgcheck-made-up") { json_answer(200, json!({"items": [], "next_cursor": null})) } else { forward() })
    });
    assert!(matches!(report.outcome("stock_read"), Some(Outcome::Red(why)) if why.contains("INVALID_CURSOR") || why.contains("400")), "{}", report.render(false));
}

#[test]
fn an_address_of_the_discovery_on_another_host_stops_the_run() {
    let report = check(|_, url, _, forward| {
        let answer = forward();
        Some(if url.ends_with("/.well-known/versekit-org.json") {
            let mut value = answer.json_body();
            value["versions"]["1"]["api_base"] = json!("https://elsewhere.example/versekit/v1");
            json_answer(200, value)
        } else {
            answer
        })
    });
    assert!(matches!(report.outcome("discovery"), Some(Outcome::Red(why)) if why.contains("api_base")), "{}", report.render(false));
    assert_eq!(report.checks.len(), 1, "nothing goes to a server the discovery does not vouch for");
}

#[test]
fn plain_http_to_another_host_is_refused_before_anything_goes_out() {
    let server = server();
    let exchange = Local { server: &server, tamper: |_, url, _, _| panic!("sent to {url}") };
    let report = run("http://orga.example.org", &exchange, &mut |_| {}, &|_| {}, &Options::default());
    assert_eq!(report.red(), ["address"]);
}
