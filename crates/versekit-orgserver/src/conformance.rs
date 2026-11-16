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

//! Whether a server follows the open org standard `versekit-org` v1 (`tools/orga-standard/v1/openapi.yaml`), checked
//! from the outside the way VerseKit talks to it: the discovery document, the login by device code up to the
//! confirmation a person does by hand, every operation of the description with its error codes, the limits of the
//! rights, `429` with `Retry-After`, the rotation of refresh tokens and the revocation.
//!
//! The same checks run as the tool `versekit-orgcheck <address>` against any server and in the tests against the
//! template server; a server that answers wrong on purpose turns them red.

use serde_json::{Value, json};

use crate::http::Response;

/// The `client_id` VerseKit sends.
pub const CLIENT_ID: &str = "versekit";
/// The rights of the standard, version 1.
const RIGHTS: [&str; 6] = crate::SCOPES;
/// The name of the good the write checks share and take back.
pub const PROBE_ITEM: &str = "VerseKit orgcheck probe";

/// One request out to the server under test.
pub trait Exchange {
    /// Sends `method` to the whole address `url` with `headers` and `body`; the error names a network failure.
    fn exchange(&self, method: &str, url: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<Response, String>;
}

/// The result of one check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Green,
    Red(String),
    Skipped(String),
}

/// One check with its label.
#[derive(Clone, Debug)]
pub struct Check {
    /// A short stable name, for tests and scripts.
    pub id: &'static str,
    pub label: String,
    pub outcome: Outcome,
}

/// All checks of a run.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub checks: Vec<Check>,
}

impl Report {
    /// Whether no check is red.
    pub fn passed(&self) -> bool {
        !self.checks.iter().any(|c| matches!(c.outcome, Outcome::Red(_)))
    }

    /// The outcome of the check `id`.
    pub fn outcome(&self, id: &str) -> Option<&Outcome> {
        self.checks.iter().find(|c| c.id == id).map(|c| &c.outcome)
    }

    /// The ids of the red checks.
    pub fn red(&self) -> Vec<&'static str> {
        self.checks.iter().filter(|c| matches!(c.outcome, Outcome::Red(_))).map(|c| c.id).collect()
    }

    /// The list for people: one line per check, green, red with the reason, or skipped with the reason.
    pub fn render(&self, german: bool) -> String {
        let mut out = String::new();
        for check in &self.checks {
            let (mark, reason) = match &check.outcome {
                Outcome::Green => (if german { "grün " } else { "green" }, String::new()),
                Outcome::Red(why) => (if german { "ROT  " } else { "RED  " }, format!(" — {why}")),
                Outcome::Skipped(why) => (if german { "übersprungen" } else { "skipped" }, format!(" — {why}")),
            };
            out.push_str(&format!("{mark}  {}{reason}\n", check.label));
        }
        let (green, red) = (
            self.checks.iter().filter(|c| c.outcome == Outcome::Green).count(),
            self.checks.iter().filter(|c| matches!(c.outcome, Outcome::Red(_))).count(),
        );
        out.push_str(&if german {
            format!("\n{green} grün, {red} rot, {} übersprungen\n", self.checks.len() - green - red)
        } else {
            format!("\n{green} green, {red} red, {} skipped\n", self.checks.len() - green - red)
        });
        out
    }
}

/// What the person confirming a login needs to know.
#[derive(Clone, Debug)]
pub struct Prompt {
    pub user_code: String,
    pub verification_uri: String,
    pub scopes: Vec<String>,
}

/// How a run behaves.
#[derive(Clone, Debug)]
pub struct Options {
    /// Also the writing operations (own stock and blueprints of the test member, taken back afterwards).
    pub write: bool,
    /// Requests sent quickly to see `429`; 0 leaves the check out.
    pub burst: usize,
    /// German labels and reasons.
    pub german: bool,
    /// The longest wait for a confirmation, in seconds.
    pub max_wait: u64,
}

impl Default for Options {
    fn default() -> Options {
        Options { write: true, burst: 300, german: false, max_wait: 600 }
    }
}

/// The endpoints of version 1 from the discovery document.
#[derive(Clone, Debug)]
struct Endpoints {
    api: String,
    device: String,
    token: String,
    revoke: Option<String>,
    scopes: Vec<String>,
}

#[derive(Clone, Debug)]
struct Tokens {
    access: String,
    refresh: String,
}

/// `scheme://host[:port]` of an address, lower case; `None` without a scheme.
fn origin_of(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default().to_ascii_lowercase();
    (!authority.is_empty() && !authority.contains('@')).then(|| (scheme.to_ascii_lowercase(), authority))
}

fn host_of(authority: &str) -> &str {
    if authority.starts_with('[') {
        authority.split_inclusive(']').next().unwrap_or(authority)
    } else {
        authority.split(':').next().unwrap_or(authority)
    }
}

fn loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// Whether `url` lies on the typed origin or a host below it, with https (or http on the own computer, as VerseKit
/// allows it).
fn allowed(base: &(String, String), url: &str) -> bool {
    let Some((scheme, authority)) = origin_of(url) else { return false };
    let (host, base_host) = (host_of(&authority), host_of(&base.1));
    let secure = scheme == "https" || (scheme == "http" && loopback(host) && base.0 == "http");
    let below = authority == base.1 || (host.ends_with(&format!(".{base_host}")) && !loopback(base_host));
    secure && below
}

fn form(pairs: &[(&str, &str)]) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect::<String>()
    };
    pairs.iter().map(|(n, v)| format!("{}={}", enc(n), enc(v))).collect::<Vec<_>>().join("&")
}

fn header<'a>(answer: &'a Response, name: &str) -> Option<&'a str> {
    answer.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

struct Run<'a> {
    base: (String, String),
    root: String,
    exchange: &'a dyn Exchange,
    confirm: &'a mut dyn FnMut(&Prompt),
    wait: &'a dyn Fn(u64),
    options: &'a Options,
    checks: Vec<Check>,
}

impl Run<'_> {
    fn say(&self, de: &str, en: &str) -> String {
        if self.options.german { de.to_string() } else { en.to_string() }
    }

    fn record(&mut self, id: &'static str, de: &str, en: &str, result: Result<(), String>) -> bool {
        let label = self.say(de, en);
        let ok = result.is_ok();
        let outcome = match result {
            Ok(()) => Outcome::Green,
            Err(why) => Outcome::Red(why),
        };
        self.checks.push(Check { id, label, outcome });
        ok
    }

    fn skip(&mut self, id: &'static str, de: &str, en: &str, why: String) {
        let label = self.say(de, en);
        self.checks.push(Check { id, label, outcome: Outcome::Skipped(why) });
    }

    fn send(&self, method: &str, url: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<Response, String> {
        self.exchange.exchange(method, url, headers, body)
    }

    fn post_form(&self, url: &str, pairs: &[(&str, &str)]) -> Result<Response, String> {
        self.send("POST", url, &[("Content-Type", "application/x-www-form-urlencoded")], form(pairs).as_bytes())
    }

    fn api(&self, method: &str, url: &str, token: Option<&str>, body: Option<&Value>, extra: &[(&str, &str)]) -> Result<Response, String> {
        let bearer = token.map(|t| format!("Bearer {t}"));
        let mut list: Vec<(&str, &str)> = vec![("Accept", "application/json")];
        if let Some(bearer) = &bearer {
            list.push(("Authorization", bearer.as_str()));
        }
        let data = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
        if body.is_some() {
            list.push(("Content-Type", "application/json"));
        }
        list.extend_from_slice(extra);
        self.send(method, url, &list, &data)
    }

    fn status(&self, answer: &Response, want: u16) -> Result<(), String> {
        if answer.status == want {
            Ok(())
        } else {
            Err(self.say("Status {g}, erwartet {w}", "status {g}, expected {w}").replace("{g}", &answer.status.to_string()).replace("{w}", &want.to_string()))
        }
    }

    /// An error of the API: the status, `application/problem+json` and the `code`.
    fn problem(&self, answer: &Response, want: u16, code: &str) -> Result<(), String> {
        self.status(answer, want)?;
        let kind = header(answer, "Content-Type").unwrap_or_default();
        if !kind.starts_with("application/problem+json") {
            return Err(self.say("Content-Type {k} statt application/problem+json", "Content-Type {k} instead of application/problem+json").replace("{k}", kind));
        }
        let got = answer.json_body().get("code").and_then(Value::as_str).unwrap_or_default().to_string();
        if got != code {
            return Err(self.say("code {g} statt {w}", "code {g} instead of {w}").replace("{g}", &got).replace("{w}", code));
        }
        Ok(())
    }

    /// An OAuth error of the login: status 400 and one of `errors`.
    fn oauth(&self, answer: &Response, errors: &[&str]) -> Result<(), String> {
        self.status(answer, 400)?;
        let got = answer.json_body().get("error").and_then(Value::as_str).unwrap_or_default().to_string();
        if errors.contains(&got.as_str()) {
            Ok(())
        } else {
            Err(self.say("error {g} statt {w}", "error {g} instead of {w}").replace("{g}", &got).replace("{w}", &errors.join(" / ")))
        }
    }

    fn discovery(&mut self) -> Option<Endpoints> {
        let url = format!("{}{}", self.root, crate::WELL_KNOWN);
        let result = (|| {
            let answer = self.send("GET", &url, &[("Accept", "application/json")], b"")?;
            self.status(&answer, 200)?;
            let root = answer.json_body();
            if text(&root, "standard") != Some("versekit-org") {
                return Err(self.say("standard ist nicht versekit-org", "standard is not versekit-org"));
            }
            if root.get("org").and_then(|o| text(o, "name")).is_none() {
                return Err(self.say("org.name fehlt", "org.name is missing"));
            }
            let entry = root.pointer("/versions/1").ok_or_else(|| self.say("versions.1 fehlt", "versions.1 is missing"))?;
            let address = |key: &str| -> Result<String, String> {
                let url = text(entry, key).ok_or_else(|| self.say("{k} fehlt", "{k} is missing").replace("{k}", key))?;
                if allowed(&self.base, url) {
                    Ok(url.trim_end_matches('/').to_string())
                } else {
                    Err(self.say(
                        "{k} liegt nicht per https auf dem eingetippten Host oder darunter: {u}",
                        "{k} is not on the typed host or below it with https: {u}",
                    )
                    .replace("{k}", key)
                    .replace("{u}", url))
                }
            };
            let revoke = match entry.get("revocation_endpoint") {
                None => None,
                Some(_) => Some(address("revocation_endpoint")?),
            };
            if entry.get("verification_uri").is_some() {
                address("verification_uri")?;
            }
            let scopes: Vec<String> = entry
                .get("scopes_supported")
                .and_then(Value::as_array)
                .ok_or_else(|| self.say("scopes_supported fehlt", "scopes_supported is missing"))?
                .iter()
                .filter_map(Value::as_str)
                // Version 1 grows by new rights; as VerseKit does, rights it does not know are passed over.
                .filter(|s| RIGHTS.contains(s))
                .map(str::to_string)
                .collect();
            if scopes.is_empty() {
                return Err(self.say(
                    "scopes_supported nennt keines der Rechte, die VerseKit kennt",
                    "scopes_supported names none of the rights VerseKit knows",
                ));
            }
            Ok(Endpoints {
                api: address("api_base")?,
                device: address("device_authorization_endpoint")?,
                token: address("token_endpoint")?,
                revoke,
                scopes,
            })
        })();
        let found = result.as_ref().ok().cloned();
        self.record("discovery", "Discovery-Dokument", "Discovery document", result.map(|_| ()));
        found
    }

    /// One login: code, then the person confirms, then the tokens. The first login also checks the waiting answers.
    fn login(&mut self, ep: &Endpoints, scopes: &[String], first: bool) -> Option<Tokens> {
        let scope = scopes.join(" ");
        let code = (|| {
            let answer = self.post_form(&ep.device, &[("client_id", CLIENT_ID), ("scope", scope.as_str())])?;
            self.status(&answer, 200)?;
            let body = answer.json_body();
            for key in ["device_code", "user_code", "verification_uri"] {
                if text(&body, key).is_none() {
                    return Err(self.say("{k} fehlt", "{k} is missing").replace("{k}", key));
                }
            }
            if !body.get("expires_in").and_then(Value::as_u64).is_some_and(|e| e > 0) {
                return Err(self.say("expires_in fehlt oder ist 0", "expires_in is missing or 0"));
            }
            let uri = text(&body, "verification_uri").unwrap_or_default();
            if !allowed(&self.base, uri) {
                return Err(self.say("verification_uri liegt woanders: {u}", "verification_uri lies elsewhere: {u}").replace("{u}", uri));
            }
            Ok(body)
        })();
        let ok = code.is_ok();
        if first {
            self.record("device_code", "Anmeldung starten (Gerätecode)", "Start a login (device code)", code.as_ref().map(|_| ()).map_err(Clone::clone));
        } else if let Err(why) = &code {
            self.record("device_code_full", "Zweite Anmeldung starten", "Start the second login", Err(why.clone()));
        }
        if !ok {
            return None;
        }
        let body = code.unwrap_or_default();
        let device = text(&body, "device_code").unwrap_or_default().to_string();
        let mut interval = body.get("interval").and_then(Value::as_u64).unwrap_or(5);
        let poll = |run: &Run| run.post_form(&ep.token, &[("grant_type", crate::DEVICE_GRANT), ("device_code", device.as_str()), ("client_id", CLIENT_ID)]);
        if first {
            let pending = poll(self).and_then(|a| self.oauth(&a, &["authorization_pending", "slow_down"]));
            self.record("token_pending", "Vor der Bestätigung: authorization_pending", "Before the confirmation: authorization_pending", pending);
            let unknown = self
                .post_form(&ep.token, &[("grant_type", crate::DEVICE_GRANT), ("device_code", "versekit-orgcheck-unknown"), ("client_id", CLIENT_ID)])
                .and_then(|a| self.oauth(&a, &["invalid_grant"]));
            self.record("token_unknown_code", "Unbekannter Gerätecode: invalid_grant", "Unknown device code: invalid_grant", unknown);
            let grant = self
                .post_form(&ep.token, &[("grant_type", "versekit-orgcheck"), ("client_id", CLIENT_ID)])
                .and_then(|a| self.oauth(&a, &["unsupported_grant_type"]));
            self.record("token_unsupported_grant", "Unbekannte Anmeldeart: unsupported_grant_type", "Unknown grant type: unsupported_grant_type", grant);
            (self.wait)(interval);
        }
        let prompt = Prompt {
            user_code: text(&body, "user_code").unwrap_or_default().to_string(),
            verification_uri: text(&body, "verification_uri").unwrap_or_default().to_string(),
            scopes: scopes.to_vec(),
        };
        (self.confirm)(&prompt);
        let mut waited = 0;
        let result = loop {
            let answer = match poll(self) {
                Ok(answer) => answer,
                Err(why) => break Err(why),
            };
            if answer.status == 200 {
                break self.tokens(&answer);
            }
            match answer.json_body().get("error").and_then(Value::as_str) {
                Some("authorization_pending") => {}
                Some("slow_down") => interval += 5,
                Some(other) => break Err(self.say("Anmeldung endet mit {e}", "the login ends with {e}").replace("{e}", other)),
                None => break Err(self.say("Status {g} ohne error", "status {g} without error").replace("{g}", &answer.status.to_string())),
            }
            if waited >= self.options.max_wait {
                break Err(self.say("niemand hat den Code rechtzeitig bestätigt", "nobody confirmed the code in time"));
            }
            (self.wait)(interval.max(1));
            waited += interval.max(1);
        };
        let tokens = result.as_ref().ok().cloned();
        let (id, de, en) = if first {
            ("tokens", "Tokens nach der Bestätigung", "Tokens after the confirmation")
        } else {
            ("tokens_full", "Tokens der zweiten Anmeldung", "Tokens of the second login")
        };
        self.record(id, de, en, result.map(|_| ()));
        tokens
    }

    fn tokens(&self, answer: &Response) -> Result<Tokens, String> {
        let body = answer.json_body();
        if !text(&body, "token_type").is_some_and(|t| t.eq_ignore_ascii_case("bearer")) {
            return Err(self.say("token_type ist nicht Bearer", "token_type is not Bearer"));
        }
        let (Some(access), Some(refresh)) = (text(&body, "access_token"), text(&body, "refresh_token")) else {
            return Err(self.say("access_token oder refresh_token fehlt", "access_token or refresh_token is missing"));
        };
        if header(answer, "Cache-Control").is_none_or(|c| !c.contains("no-store")) {
            return Err(self.say("Cache-Control: no-store fehlt", "Cache-Control: no-store is missing"));
        }
        if body.get("expires_in").and_then(Value::as_u64).is_some_and(|e| e > 3600) {
            return Err(self.say("expires_in über einer Stunde", "expires_in above one hour"));
        }
        Ok(Tokens { access: access.to_string(), refresh: refresh.to_string() })
    }

    fn me(&self, ep: &Endpoints, token: &str) -> Result<Value, String> {
        let answer = self.api("GET", &format!("{}/me", ep.api), Some(token), None, &[])?;
        self.status(&answer, 200)?;
        let body = answer.json_body();
        if body.pointer("/member/name").and_then(Value::as_str).is_none() || body.get("scopes").and_then(Value::as_array).is_none() {
            return Err(self.say("member.name oder scopes fehlt", "member.name or scopes is missing"));
        }
        Ok(body)
    }

    /// A list of the API: status 200, the list under `key`, `next_cursor` as text or null; with more than one entry
    /// the next page is read, and a made-up cursor is refused.
    fn list(&self, ep: &Endpoints, token: &str, path: &str, key: &str) -> Result<Vec<Value>, String> {
        let url = format!("{}{path}", ep.api);
        let answer = self.api("GET", &url, Some(token), None, &[])?;
        self.status(&answer, 200)?;
        let body = answer.json_body();
        let Some(items) = body.get(key).and_then(Value::as_array).cloned() else {
            return Err(self.say("{k} ist keine Liste", "{k} is no list").replace("{k}", key));
        };
        if !matches!(body.get("next_cursor"), Some(Value::String(_) | Value::Null) | None) {
            return Err(self.say("next_cursor ist weder Text noch null", "next_cursor is neither text nor null"));
        }
        let first = self.api("GET", &format!("{url}?limit=1"), Some(token), None, &[])?;
        self.status(&first, 200)?;
        if items.len() > 1 {
            let Some(cursor) = first.json_body().get("next_cursor").and_then(Value::as_str).map(str::to_string) else {
                return Err(self.say("bei limit=1 fehlt next_cursor", "next_cursor is missing with limit=1"));
            };
            let next = self.api("GET", &format!("{url}?limit=1&cursor={}", form(&[("", cursor.as_str())]).trim_start_matches('=')), Some(token), None, &[])?;
            self.status(&next, 200)?;
        }
        let wrong = self.api("GET", &format!("{url}?cursor=versekit-orgcheck-made-up"), Some(token), None, &[])?;
        self.problem(&wrong, 400, "INVALID_CURSOR")?;
        Ok(items)
    }

    fn check_lists(&mut self, ep: &Endpoints, token: &str, granted: &[String]) -> Vec<Value> {
        let has = |s: &str| granted.iter().any(|g| g == s);
        let mut orders = Vec::new();
        for (scope, path, key, id, de, en) in [
            ("stock:read", "/stock", "items", "stock_read", "Bestand der Orga lesen", "Read the org's stock"),
            ("blueprints:read", "/blueprints", "blueprints", "blueprints_read", "Baupläne der Orga lesen", "Read the org's blueprints"),
            ("orders:read", "/orders", "orders", "orders_read", "Aufträge lesen", "Read the orders"),
        ] {
            if !has(scope) {
                self.skip(id, de, en, self.say("Recht nicht angeboten", "right not offered"));
                continue;
            }
            let result = self.list(ep, token, path, key);
            if key == "orders"
                && let Ok(list) = &result
            {
                orders = list.clone();
            }
            let checked = result.and_then(|list| {
                if key == "orders" {
                    for order in &list {
                        let fine = text(order, "id").is_some()
                            && text(order, "title").is_some()
                            && matches!(text(order, "status"), Some("open" | "done" | "cancelled"))
                            && order.get("items").and_then(Value::as_array).is_some();
                        if !fine {
                            return Err(self.say("ein Auftrag ohne id, title, status oder items", "an order without id, title, status or items"));
                        }
                    }
                }
                Ok(())
            });
            self.record(id, de, en, checked);
        }
        orders
    }

    fn check_writes(&mut self, ep: &Endpoints, token: &str, granted: &[String]) {
        let has = |s: &str| granted.iter().any(|g| g == s);
        for (scope, path, field, good, back, id, de, en) in [
            (
                "stock:write",
                "/stock/mine",
                "items_stored",
                json!({ "items": [{ "item": PROBE_ITEM, "quantity": 1 }] }),
                json!({ "items": [] }),
                "stock_write",
                "Eigenen Bestand teilen (und zurücknehmen)",
                "Share own stock (and take it back)",
            ),
            (
                "blueprints:write",
                "/blueprints/mine",
                "blueprints_stored",
                json!({ "blueprints": [PROBE_ITEM] }),
                json!({ "blueprints": [] }),
                "blueprints_write",
                "Eigene Baupläne teilen (und zurücknehmen)",
                "Share own blueprints (and take them back)",
            ),
        ] {
            if !self.options.write {
                self.skip(id, de, en, self.say("--no-write: nichts geschrieben", "--no-write: nothing written"));
                continue;
            }
            if !has(scope) {
                self.skip(id, de, en, self.say("Recht nicht angeboten", "right not offered"));
                continue;
            }
            let url = format!("{}{path}", ep.api);
            let result = (|| {
                let answer = self.api("PUT", &url, Some(token), Some(&good), &[])?;
                self.status(&answer, 200)?;
                if answer.json_body().get(field).and_then(Value::as_u64) != Some(1) {
                    return Err(self.say("{f} ist nicht 1", "{f} is not 1").replace("{f}", field));
                }
                let bearer = format!("Bearer {token}");
                let broken = self.send("PUT", &url, &[("Authorization", bearer.as_str()), ("Content-Type", "application/json")], b"{")?;
                self.problem(&broken, 400, "INVALID_BODY")?;
                let answer = self.api("PUT", &url, Some(token), Some(&back), &[])?;
                self.status(&answer, 200)
            })();
            self.record(id, de, en, result);
        }
    }

    fn check_deliveries(&mut self, ep: &Endpoints, token: &str, granted: &[String], orders: &[Value]) {
        let (id, de, en) = ("deliveries_errors", "Lieferungen: Fehlercodes", "Deliveries: error codes");
        if !granted.iter().any(|g| g == "orders:write") {
            self.skip(id, de, en, self.say("Recht nicht angeboten", "right not offered"));
            return;
        }
        let result = (|| {
            let unknown = format!("{}/orders/versekit-orgcheck-unknown/deliveries", ep.api);
            let body = json!({ "items": [{ "item": PROBE_ITEM, "quantity": 1 }] });
            let answer = self.api("POST", &unknown, Some(token), Some(&body), &[])?;
            self.problem(&answer, 400, "IDEMPOTENCY_KEY_MISSING")?;
            let answer = self.api("POST", &unknown, Some(token), Some(&body), &[("Idempotency-Key", "versekit-orgcheck-1")])?;
            self.problem(&answer, 404, "NOT_FOUND")?;
            if let Some(open) = orders.iter().find(|o| text(o, "status") == Some("open")).and_then(|o| text(o, "id")) {
                let url = format!("{}/orders/{}/deliveries", ep.api, form(&[("", open)]).trim_start_matches('='));
                let wrong = self.api("POST", &url, Some(token), Some(&body), &[("Idempotency-Key", "versekit-orgcheck-2")])?;
                self.problem(&wrong, 422, "ITEM_NOT_IN_ORDER")?;
            }
            Ok(())
        })();
        self.record(id, de, en, result);
    }

    fn check_refresh_and_revoke(&mut self, ep: &Endpoints, old: &Tokens) -> Option<Tokens> {
        let refreshed = (|| {
            let answer = self.post_form(&ep.token, &[("grant_type", "refresh_token"), ("refresh_token", old.refresh.as_str()), ("client_id", CLIENT_ID)])?;
            self.status(&answer, 200)?;
            let new = self.tokens(&answer)?;
            if new.refresh == old.refresh {
                return Err(self.say("der Refresh-Token wechselt nicht", "the refresh token does not rotate"));
            }
            self.me(ep, &new.access)?;
            let again = self.post_form(&ep.token, &[("grant_type", "refresh_token"), ("refresh_token", old.refresh.as_str()), ("client_id", CLIENT_ID)])?;
            self.oauth(&again, &["invalid_grant"]).map_err(|why| self.say("der alte Refresh-Token gilt noch: {w}", "the old refresh token still works: {w}").replace("{w}", &why))?;
            Ok(new)
        })();
        let new = refreshed.as_ref().ok().cloned();
        self.record("refresh_rotation", "Erneuern mit Wechsel des Refresh-Tokens", "Refresh with rotation of the refresh token", refreshed.map(|_| ()));
        new
    }

    fn check_revoke(&mut self, ep: &Endpoints, tokens: &Tokens) {
        let (id, de, en) = ("revoke", "Widerruf beendet die Anmeldung", "Revocation ends the login");
        let Some(endpoint) = ep.revoke.clone() else {
            self.skip(id, de, en, self.say("kein revocation_endpoint (erlaubt)", "no revocation_endpoint (allowed)"));
            return;
        };
        let result = (|| {
            let answer = self.post_form(&endpoint, &[("token", tokens.refresh.as_str()), ("token_type_hint", "refresh_token"), ("client_id", CLIENT_ID)])?;
            self.status(&answer, 200)?;
            let me = self.api("GET", &format!("{}/me", ep.api), Some(&tokens.access), None, &[])?;
            self.problem(&me, 401, "UNAUTHORIZED").map_err(|why| self.say("der Access-Token gilt nach dem Widerruf noch: {w}", "the access token still works after the revocation: {w}").replace("{w}", &why))?;
            let again = self.post_form(&ep.token, &[("grant_type", "refresh_token"), ("refresh_token", tokens.refresh.as_str()), ("client_id", CLIENT_ID)])?;
            self.oauth(&again, &["invalid_grant"])
        })();
        self.record(id, de, en, result);
    }

    fn check_burst(&mut self, ep: &Endpoints, token: &str) {
        let (id, de, en) = ("rate_limit", "Bremse: 429 mit Retry-After", "Rate limit: 429 with Retry-After");
        if self.options.burst == 0 {
            self.skip(id, de, en, self.say("--burst 0", "--burst 0"));
            return;
        }
        let url = format!("{}/me", ep.api);
        let mut result = Err(self.say("kein 429 nach {n} schnellen Anfragen", "no 429 after {n} quick requests").replace("{n}", &self.options.burst.to_string()));
        for _ in 0..self.options.burst {
            match self.api("GET", &url, Some(token), None, &[]) {
                Ok(answer) if answer.status == 429 => {
                    result = self.problem(&answer, 429, "RATE_LIMITED").and_then(|()| {
                        match header(&answer, "Retry-After").and_then(|v| v.trim().parse::<u64>().ok()) {
                            Some(seconds) if seconds >= 1 => Ok(()),
                            _ => Err(self.say("Retry-After fehlt oder ist keine Zahl ab 1", "Retry-After is missing or no number from 1")),
                        }
                    });
                    break;
                }
                Ok(_) => {}
                Err(why) => {
                    result = Err(why);
                    break;
                }
            }
        }
        self.record(id, de, en, result);
    }
}

/// Runs every check against the server at `address` (the address a member types). `confirm` is called when a person
/// has to confirm a code on the org's page; `wait` waits the given seconds between two polls.
pub fn run(address: &str, exchange: &dyn Exchange, confirm: &mut dyn FnMut(&Prompt), wait: &dyn Fn(u64), options: &Options) -> Report {
    let root = address.trim().trim_end_matches('/').to_string();
    let base = origin_of(&root).unwrap_or_default();
    let mut run = Run { base, root, exchange, confirm, wait, options, checks: Vec::new() };
    let typed_ok = origin_of(&run.root).is_some_and(|(scheme, authority)| scheme == "https" || (scheme == "http" && loopback(host_of(&authority))));
    if !typed_ok {
        run.record("address", "Adresse", "Address", Err(run.say("nur https://host (http nur auf dem eigenen Rechner)", "only https://host (http only on the own computer)")));
        return Report { checks: run.checks };
    }
    let Some(ep) = run.discovery() else { return Report { checks: run.checks } };

    let bad_scope = run
        .post_form(&ep.device, &[("client_id", CLIENT_ID), ("scope", "versekit-orgcheck:unknown")])
        .and_then(|a| run.oauth(&a, &["invalid_scope"]));
    run.record("device_invalid_scope", "Nur unbekannte Rechte: invalid_scope", "Only unknown rights: invalid_scope", bad_scope);

    let reads = ["stock:read", "blueprints:read", "orders:read"];
    let offered: Vec<&str> = reads.iter().copied().filter(|s| ep.scopes.iter().any(|x| x == s)).collect();
    let limited: Vec<String> = vec![offered.first().copied().unwrap_or(ep.scopes[0].as_str()).to_string()];
    let Some(small) = run.login(&ep, &limited, true) else { return Report { checks: run.checks } };

    let me = run.me(&ep, &small.access).and_then(|body| {
        let scopes: Vec<&str> = body.get("scopes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
        if scopes.iter().any(|s| !limited.iter().any(|l| l == s)) {
            Err(run.say("/me nennt Rechte, die nicht gegeben wurden", "/me names rights that were not granted"))
        } else {
            Ok(())
        }
    });
    run.record("me", "/me mit Mitglied und Rechten", "/me with member and rights", me);

    let (id, de, en) = ("scope_limit", "Nicht gegebenes Recht: 403 SCOPE_MISSING", "Right not granted: 403 SCOPE_MISSING");
    match offered.get(1) {
        Some(other) => {
            let path = match *other {
                "blueprints:read" => "/blueprints",
                _ => "/orders",
            };
            let result = run.api("GET", &format!("{}{path}", ep.api), Some(&small.access), None, &[]).and_then(|a| run.problem(&a, 403, "SCOPE_MISSING"));
            run.record(id, de, en, result);
        }
        None => run.skip(id, de, en, run.say("nur ein lesendes Recht angeboten", "only one reading right offered")),
    }

    let unauthorized = (|| {
        let none = run.api("GET", &format!("{}/me", ep.api), None, None, &[])?;
        run.problem(&none, 401, "UNAUTHORIZED")?;
        if !header(&none, "WWW-Authenticate").is_some_and(|h| h.to_ascii_lowercase().starts_with("bearer")) {
            return Err(run.say("WWW-Authenticate: Bearer fehlt", "WWW-Authenticate: Bearer is missing"));
        }
        let made_up = run.api("GET", &format!("{}/me", ep.api), Some("versekit-orgcheck-made-up"), None, &[])?;
        run.problem(&made_up, 401, "UNAUTHORIZED")
    })();
    run.record("unauthorized", "Ohne oder mit falschem Token: 401 UNAUTHORIZED", "Without or with a wrong token: 401 UNAUTHORIZED", unauthorized);

    let method = run.api("DELETE", &format!("{}/me", ep.api), Some(&small.access), None, &[]).and_then(|a| run.problem(&a, 405, "METHOD_NOT_ALLOWED"));
    run.record("method_not_allowed", "Falsche Methode: 405 METHOD_NOT_ALLOWED", "Wrong method: 405 METHOD_NOT_ALLOWED", method);

    // The rotation is checked with the first login: a server that ends the whole login when an old refresh token
    // comes again (as RFC 9700 recommends) would otherwise stop the tokens of the checks after it.
    let small = run.check_refresh_and_revoke(&ep, &small).unwrap_or(small);
    let all = ep.scopes.clone();
    let Some(full) = run.login(&ep, &all, false) else { return Report { checks: run.checks } };
    let orders = run.check_lists(&ep, &full.access, &all);
    run.check_writes(&ep, &full.access, &all);
    run.check_deliveries(&ep, &full.access, &all, &orders);
    run.check_burst(&ep, &full.access);
    run.check_revoke(&ep, &full);
    if let Some(endpoint) = ep.revoke.clone() {
        let _ = run.post_form(&endpoint, &[("token", small.refresh.as_str()), ("client_id", CLIENT_ID)]);
    }
    run.skip(
        "maintenance",
        "Wartung: 503 MAINTENANCE mit Retry-After",
        "Maintenance: 503 MAINTENANCE with Retry-After",
        run.say("nur prüfbar, während der Server gerade wartet", "only checkable while the server is in maintenance"),
    );
    Report { checks: run.checks }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_of_the_discovery_lie_on_the_typed_host_or_below() {
        let base = origin_of("https://orga.example.org").unwrap();
        assert!(allowed(&base, "https://orga.example.org/versekit/v1"));
        assert!(allowed(&base, "https://api.orga.example.org/v1"));
        assert!(!allowed(&base, "http://orga.example.org/v1"));
        assert!(!allowed(&base, "https://evilorga.example.org/v1"));
        assert!(!allowed(&base, "https://example.org/v1"));
        let local = origin_of("http://127.0.0.1:8110").unwrap();
        assert!(allowed(&local, "http://127.0.0.1:8110/versekit/v1"));
        assert!(!allowed(&local, "http://127.0.0.1:9000/versekit/v1"));
    }
}
