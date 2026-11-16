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

//! The members' portal at `/portal` against a stand-in for Discord: off without the Discord settings; the login sends
//! `state`, PKCE and the exact redirect address and takes back only its own `state` from the same browser, once; only
//! members of the org's Discord server get in, and only with a role; every page checks the role, every form the CSRF
//! token and the origin; sessions end after 8 hours, after 2 hours unused and at log-out; blocking acts at once;
//! nothing secret lands in the state file.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use versekit_orgserver::conformance::Exchange;
use versekit_orgserver::http::{Request, Response, parse_pairs, value};
use versekit_orgserver::{Config, Server, portal};

const PASSWORD: &str = "orga-leitung-passwort-42";
const CLIENT: &str = "900000000000000002";
const SECRET: &str = "geheimnis-der-discord-app-0123";
const GUILD: &str = "900000000000000001";
const LEAD_ROLE: &str = "900000000000000010";
const LOGISTICS_ROLE: &str = "900000000000000011";
const MEMBER_ROLE: &str = "900000000000000012";
const GUEST_ROLE: &str = "900000000000000013";
const VEGA: &str = "800000000000000001";
const NOVA: &str = "800000000000000002";
const ORION: &str = "800000000000000003";
const GAST: &str = "800000000000000004";
/// 2027-01-15 08:00 UTC.
const T0: u64 = 1_800_000_000;

/// One request to Discord: method, address, headers, body.
type Call = (String, String, Vec<(String, String)>, String);

/// Discord as the portal sees it: answers by address and keeps every request.
struct FakeDiscord {
    calls: Mutex<Vec<Call>>,
    token: Mutex<(u16, Value)>,
    member: Mutex<(u16, Value)>,
}

impl FakeDiscord {
    fn new() -> Arc<FakeDiscord> {
        Arc::new(FakeDiscord {
            calls: Mutex::new(Vec::new()),
            token: Mutex::new((200, json!({ "access_token": "discord-zugang-123", "token_type": "Bearer", "expires_in": 604800, "scope": "identify guilds.members.read" }))),
            member: Mutex::new((404, json!({ "message": "Unknown Guild", "code": 10004 }))),
        })
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    fn header(call: &Call, name: &str) -> String {
        call.2.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()).unwrap_or_default()
    }
}

impl Exchange for FakeDiscord {
    fn exchange(&self, method: &str, url: &str, headers: &[(&str, &str)], body: &[u8]) -> Result<Response, String> {
        let headers = headers.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect();
        self.calls.lock().unwrap().push((method.into(), url.into(), headers, String::from_utf8_lossy(body).into_owned()));
        let (status, body) = if url == portal::TOKEN_URL {
            self.token.lock().unwrap().clone()
        } else if url == format!("{}/users/@me/guilds/{GUILD}/member", portal::API_URL) {
            self.member.lock().unwrap().clone()
        } else if url == portal::REVOKE_URL {
            (200, json!({}))
        } else {
            (404, json!({}))
        };
        Ok(Response::json(status, &body))
    }
}

/// A member of the Discord server with these roles.
fn on_server(id: &str, name: &str, roles: &[&str]) -> Value {
    json!({ "user": { "id": id, "username": name.to_lowercase(), "global_name": name }, "nick": null, "roles": roles, "pending": false })
}

fn config() -> Config {
    Config::new("https://orga.example.org", "Nebelorga", "NBL")
        .with_member("vega", "Vega", "schluessel-von-vega-0123")
        .with_order(json!({"id": "a1", "title": "Erz für die Werft", "items": [{"item": "Quantanium", "quantity": 10, "unit": "scu"}]}))
        .with_admin_password(PASSWORD)
        .unwrap()
        .with_discord(CLIENT, SECRET, GUILD)
        .unwrap()
}

fn server() -> (Server, Arc<FakeDiscord>) {
    let fake = FakeDiscord::new();
    (Server::new(config()).with_outbound(fake.clone()), fake)
}

fn text(answer: &Response) -> String {
    String::from_utf8(answer.body.clone()).unwrap()
}

fn headers<'a>(answer: &'a Response, name: &str) -> Vec<&'a str> {
    answer.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
}

fn location(answer: &Response) -> String {
    headers(answer, "Location").first().map(|s| s.to_string()).unwrap_or_default()
}

fn urlencode(pairs: &[(&str, &str)]) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect::<String>()
    };
    pairs.iter().map(|(n, v)| format!("{}={}", enc(n), enc(v))).collect::<Vec<_>>().join("&")
}

fn hidden(page: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let start = page.find(&marker).unwrap_or_else(|| panic!("no field {name} in {page}")) + marker.len();
    page[start..].split('"').next().unwrap().to_string()
}

fn query_of(url: &str) -> Vec<(String, String)> {
    parse_pairs(url.split_once('?').map(|(_, q)| q).unwrap_or_default())
}

/// A browser with its cookies, its language and the clock of the server.
struct Browser<'a> {
    server: &'a Server,
    cookies: Vec<(String, String)>,
    language: &'static str,
    now: u64,
    extra: Vec<(&'static str, &'static str)>,
}

impl<'a> Browser<'a> {
    fn new(server: &'a Server) -> Browser<'a> {
        Browser { server, cookies: Vec::new(), language: "de-DE,de;q=0.9", now: T0, extra: Vec::new() }
    }

    fn send(&mut self, method: &str, target: &str, body: &str, extra: &[(&str, &str)]) -> Response {
        let cookie = self.cookies.iter().map(|(n, v)| format!("{n}={v}")).collect::<Vec<_>>().join("; ");
        let mut list: Vec<(&str, &str)> =
            vec![("Host", "orga.example.org"), ("Accept-Language", self.language), ("X-Forwarded-Proto", "https")];
        if !cookie.is_empty() {
            list.push(("Cookie", cookie.as_str()));
        }
        if method == "POST" {
            list.push(("Content-Type", "application/x-www-form-urlencoded"));
        }
        list.extend_from_slice(&self.extra);
        list.extend_from_slice(extra);
        let answer = self.server.handle_at(&Request::new(method, target, &list, body.as_bytes()), self.now);
        for value in headers(&answer, "Set-Cookie") {
            let pair = value.split(';').next().unwrap();
            let (name, content) = pair.split_once('=').unwrap();
            self.cookies.retain(|(n, _)| n != name);
            if !content.is_empty() && !value.contains("Max-Age=0") {
                self.cookies.push((name.to_string(), content.to_string()));
            }
        }
        answer
    }

    fn get(&mut self, target: &str) -> Response {
        self.send("GET", target, "", &[])
    }

    fn post(&mut self, target: &str, pairs: &[(&str, &str)]) -> Response {
        self.send("POST", target, &urlencode(pairs), &[])
    }

    fn cookie(&self, name: &str) -> Option<String> {
        self.cookies.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone())
    }

    /// Starts a login and comes back from Discord with `code` and the `state` it was given.
    fn through_discord(&mut self, fake: &FakeDiscord, who: Value) -> Response {
        *fake.member.lock().unwrap() = (200, who);
        let go = self.get("/portal/login");
        let state = value(&query_of(&location(&go)), "state").unwrap().to_string();
        self.get(&format!("/portal/callback?code=abcDEF123&state={state}&iss=https%3A%2F%2Fdiscord.com"))
    }

    /// The CSRF token of the session, read from a page of the portal.
    fn csrf(&mut self, base: &str) -> String {
        hidden(&text(&self.get(base)), "csrf")
    }

    /// Logs in through the admin password and maps the Discord roles.
    fn map_roles(&mut self, everyone: bool) {
        let form = text(&self.get("/admin/login"));
        let token = hidden(&form, "csrf");
        let answer = self.post("/admin/login", &[("csrf", token.as_str()), ("password", PASSWORD)]);
        assert_eq!(answer.status, 303, "the admin password lets in");
        let csrf = self.csrf("/admin");
        let mut form = vec![("csrf", csrf.as_str()), ("lead", LEAD_ROLE), ("logistics", LOGISTICS_ROLE), ("member", MEMBER_ROLE)];
        if everyone {
            form.push(("everyone", "1"));
        }
        let saved = self.post("/admin/settings", &form);
        assert_eq!(saved.status, 303, "{}", text(&saved));
    }
}

/// Nothing from another server: no script, no stylesheet, font or picture from elsewhere, no style attribute.
fn loads_nothing_from_elsewhere(page: &str) -> bool {
    let own = page.replacen("<link rel=\"stylesheet\" href=\"/assets/look.css?v=", "", 1);
    !["<script", "src=\"http", "src='http", "url(http", "url(//", "@import", "<link ", "<style", " style="].iter().any(|p| own.contains(p))
}

/// A server whose roles are mapped, with a browser of the lead logged in.
fn with_lead<'a>(server: &'a Server, fake: &FakeDiscord) -> Browser<'a> {
    Browser::new(server).map_roles(false);
    let mut lead = Browser::new(server);
    let answer = lead.through_discord(fake, on_server(VEGA, "Vega", &[LEAD_ROLE]));
    assert_eq!(answer.status, 200, "{}", text(&answer));
    lead
}

#[test]
fn without_the_discord_settings_the_portal_is_off() {
    let server = Server::new(Config::new("https://orga.example.org", "Nebelorga", "NBL"));
    let mut browser = Browser::new(&server);
    for path in ["/portal", "/portal/login", "/portal/callback?code=x&state=y", "/portal/orders"] {
        let answer = browser.get(path);
        assert_eq!(answer.status, 404, "{path}");
        assert!(text(&answer).contains("Mitglieder-Portal ist aus"), "{path}");
        assert!(headers(&answer, "Set-Cookie").is_empty(), "{path}: no cookie without the portal");
    }
    assert!(!text(&browser.get("/")).contains("/portal"), "the status page offers no portal");
    let (on, _) = self::server();
    assert!(text(&Browser::new(&on).get("/")).contains("href=\"/portal\""), "with Discord the status page leads to the portal");

    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| pairs.iter().find(|(n, _)| *n == name).map(|(_, v)| v.to_string())
    };
    let base = Config::new("https://orga.example.org", "O", "O");
    let half = base.clone().with_env(env(&[("VERSEKIT_ORG_DISCORD_CLIENT_ID", CLIENT), ("VERSEKIT_ORG_DISCORD_GUILD_ID", GUILD)]));
    let why = half.unwrap_err();
    assert!(why.contains("VERSEKIT_ORG_DISCORD_CLIENT_SECRET") && !why.contains("CLIENT_ID,"), "{why}");
    let full = base
        .clone()
        .with_env(env(&[
            ("VERSEKIT_ORG_DISCORD_CLIENT_ID", CLIENT),
            ("VERSEKIT_ORG_DISCORD_CLIENT_SECRET", SECRET),
            ("VERSEKIT_ORG_DISCORD_GUILD_ID", GUILD),
        ]))
        .unwrap();
    assert!(full.discord.is_some());
    assert!(!format!("{full:?}").contains(SECRET), "the secret never shows");
    let bad = base.with_env(env(&[
        ("VERSEKIT_ORG_DISCORD_CLIENT_ID", "meine-app"),
        ("VERSEKIT_ORG_DISCORD_CLIENT_SECRET", SECRET),
        ("VERSEKIT_ORG_DISCORD_GUILD_ID", GUILD),
    ]));
    assert!(bad.unwrap_err().contains("CLIENT_ID"));
}

#[test]
fn the_login_sends_state_pkce_and_the_exact_address_and_keeps_nothing_of_discord() {
    let (server, fake) = server();
    Browser::new(&server).map_roles(false);
    let mut browser = Browser::new(&server);
    let go = browser.get("/portal/login");
    assert_eq!(go.status, 303);
    let target = location(&go);
    assert!(target.starts_with("https://discord.com/oauth2/authorize?"), "{target}");
    let query = query_of(&target);
    assert_eq!(value(&query, "response_type"), Some("code"));
    assert_eq!(value(&query, "client_id"), Some(CLIENT));
    assert_eq!(value(&query, "scope"), Some("identify guilds.members.read"));
    assert_eq!(value(&query, "redirect_uri"), Some("https://orga.example.org/portal/callback"));
    assert_eq!(value(&query, "code_challenge_method"), Some("S256"));
    let state = value(&query, "state").unwrap().to_string();
    assert_eq!(state.len(), 64);
    let challenge = value(&query, "code_challenge").unwrap().to_string();
    let login_cookie = headers(&go, "Set-Cookie").into_iter().find(|c| c.starts_with("__Host-vk_portal_login=")).expect("a login cookie");
    for part in ["HttpOnly", "Secure", "Path=/", "SameSite=Lax", "Max-Age=600"] {
        assert!(login_cookie.contains(part), "{login_cookie}: {part}");
    }

    *fake.member.lock().unwrap() = (200, on_server(VEGA, "Vega", &[LEAD_ROLE]));
    let back = browser.get(&format!("/portal/callback?code=abcDEF123&state={state}"));
    assert_eq!(back.status, 200, "{}", text(&back));
    assert!(text(&back).contains("http-equiv=\"refresh\" content=\"0;url=/portal\""), "a page of this server leads on");
    let session = headers(&back, "Set-Cookie").into_iter().find(|c| c.starts_with("__Host-vk_portal=")).expect("a session cookie");
    for part in ["HttpOnly", "Secure", "Path=/", "SameSite=Strict", "Max-Age=28800"] {
        assert!(session.contains(part), "{session}: {part}");
    }
    assert!(browser.cookie("__Host-vk_portal_login").is_none(), "the login cookie is cleared");

    let calls = fake.calls();
    assert_eq!(calls.len(), 3, "token, member, revoke");
    let token = &calls[0];
    assert_eq!((token.0.as_str(), token.1.as_str()), ("POST", portal::TOKEN_URL));
    let auth = FakeDiscord::header(token, "Authorization");
    assert_eq!(auth, format!("Basic {}", portal::base64(format!("{CLIENT}:{SECRET}").as_bytes(), false)));
    let form = parse_pairs(&token.3);
    assert_eq!(value(&form, "grant_type"), Some("authorization_code"));
    assert_eq!(value(&form, "code"), Some("abcDEF123"));
    assert_eq!(value(&form, "redirect_uri"), Some("https://orga.example.org/portal/callback"));
    let verifier = value(&form, "code_verifier").unwrap();
    assert!(verifier.len() >= 43, "{verifier}");
    assert_eq!(portal::challenge(verifier), challenge, "the verifier fits the challenge sent before");
    assert!(!token.3.contains(SECRET), "the secret goes in the header only");
    let member = &calls[1];
    assert_eq!(member.1, format!("{}/users/@me/guilds/{GUILD}/member", portal::API_URL));
    assert_eq!(FakeDiscord::header(member, "Authorization"), "Bearer discord-zugang-123");
    let revoke = &calls[2];
    assert_eq!(revoke.1, portal::REVOKE_URL);
    assert!(revoke.3.contains("token=discord-zugang-123"), "the Discord token is revoked at once");

    let home = browser.get("/portal");
    assert_eq!(home.status, 200);
    let page = text(&home);
    assert!(page.contains("Vega · Leitung"), "{page}");
    assert!(loads_nothing_from_elsewhere(&page));
    let policy = headers(&home, "Content-Security-Policy").join(" ");
    assert!(policy.contains("default-src 'none'") && policy.contains("form-action 'self'"), "{policy}");
    browser.language = "en-US,en;q=0.9";
    assert!(text(&browser.get("/portal")).contains("Vega · Lead"), "English after the browser");
}

#[test]
fn only_the_own_state_from_the_same_browser_comes_back_and_only_once() {
    let (server, fake) = server();
    Browser::new(&server).map_roles(true);
    *fake.member.lock().unwrap() = (200, on_server(NOVA, "Nova", &[]));
    let mut browser = Browser::new(&server);
    let state = value(&query_of(&location(&browser.get("/portal/login"))), "state").unwrap().to_string();

    let unknown = browser.get(&format!("/portal/callback?code=abc&state={}", "0".repeat(64)));
    assert_eq!(unknown.status, 400, "an unknown state");
    let mut other = Browser::new(&server);
    let foreign = other.get(&format!("/portal/callback?code=abc&state={state}"));
    assert_eq!(foreign.status, 400, "the state of another browser");
    assert!(fake.calls().is_empty(), "Discord was never asked");
    let used = browser.get(&format!("/portal/callback?code=abc&state={state}"));
    assert_eq!(used.status, 400, "the state was used up by the try of the other browser");

    let state = value(&query_of(&location(&browser.get("/portal/login"))), "state").unwrap().to_string();
    let wrong_issuer = browser.get(&format!("/portal/callback?code=abc&state={state}&iss=https%3A%2F%2Fevil.example"));
    assert_eq!(wrong_issuer.status, 400, "another issuer");
    assert!(fake.calls().is_empty());

    let state = value(&query_of(&location(&browser.get("/portal/login"))), "state").unwrap().to_string();
    let cancelled = browser.get(&format!("/portal/callback?error=access_denied&state={state}"));
    assert_eq!(cancelled.status, 400);
    assert!(text(&cancelled).contains("abgebrochen"));

    let state = value(&query_of(&location(&browser.get("/portal/login"))), "state").unwrap().to_string();
    browser.now += portal::LOGIN_SECONDS + 1;
    assert_eq!(browser.get(&format!("/portal/callback?code=abc&state={state}")).status, 400, "an expired login");
    assert!(fake.calls().is_empty(), "Discord was never asked");

    let state = value(&query_of(&location(&browser.get("/portal/login"))), "state").unwrap().to_string();
    let tie = browser.cookie("__Host-vk_portal_login").expect("the cookie of the login");
    let good = browser.get(&format!("/portal/callback?code=abc&state={state}"));
    assert_eq!(good.status, 200, "{}", text(&good));
    // A copy of the cookie keeps the tie: the state alone must refuse the second use.
    browser.cookies.push(("__Host-vk_portal_login".into(), tie));
    let again = browser.get(&format!("/portal/callback?code=abc&state={state}"));
    assert_eq!(again.status, 400, "a state counts once, even with the cookie of its browser");
    assert_eq!(fake.calls().len(), 3, "one exchange only");
}

#[test]
fn only_members_of_the_discord_server_get_in_and_failures_keep_them_out() {
    let (server, fake) = server();
    Browser::new(&server).map_roles(true);
    let mut browser = Browser::new(&server);
    *fake.member.lock().unwrap() = (404, json!({ "message": "Unknown Guild", "code": 10004 }));
    let state = value(&query_of(&location(&browser.get("/portal/login"))), "state").unwrap().to_string();
    let outside = browser.get(&format!("/portal/callback?code=abc&state={state}"));
    assert_eq!(outside.status, 403);
    assert!(text(&outside).contains("nicht auf dem Discord-Server"));
    assert!(browser.cookie("__Host-vk_portal").is_none());

    let mut pending = on_server(ORION, "Orion", &[MEMBER_ROLE]);
    pending["pending"] = json!(true);
    let rules = Browser::new(&server).through_discord(&fake, pending);
    assert_eq!(rules.status, 403, "the rules of the server not yet accepted");

    *fake.token.lock().unwrap() = (400, json!({ "error": "invalid_grant" }));
    let mut refused = Browser::new(&server);
    assert_eq!(refused.through_discord(&fake, on_server(NOVA, "Nova", &[MEMBER_ROLE])).status, 502);
    assert!(refused.cookie("__Host-vk_portal").is_none());
    *fake.token.lock().unwrap() = (429, json!({ "retry_after": 2.0 }));
    assert_eq!(Browser::new(&server).through_discord(&fake, on_server(NOVA, "Nova", &[MEMBER_ROLE])).status, 503);
    *fake.token.lock().unwrap() = (200, json!({ "access_token": "x y", "token_type": "Bearer" }));
    assert_eq!(Browser::new(&server).through_discord(&fake, on_server(NOVA, "Nova", &[MEMBER_ROLE])).status, 502, "a token that does not fit");
    let last = fake.calls().last().cloned().unwrap();
    assert_eq!((last.1.as_str(), last.3.as_str()), (portal::REVOKE_URL, "token=x%20y&token_type_hint=access_token"), "is revoked all the same");
    *fake.token.lock().unwrap() = (200, json!({ "access_token": "discord-zugang-456", "token_type": "mac" }));
    assert_eq!(Browser::new(&server).through_discord(&fake, on_server(NOVA, "Nova", &[MEMBER_ROLE])).status, 502, "no bearer token");
    let last = fake.calls().last().cloned().unwrap();
    assert!(last.1 == portal::REVOKE_URL && last.3.contains("discord-zugang-456"), "is revoked too");

    let mut admin = Browser::new(&server);
    admin.map_roles(true);
    let access = text(&admin.get("/admin/access"));
    assert!(!access.contains(ORION) && !access.contains(NOVA) && !access.contains(GAST), "nobody outside is kept");
}

#[test]
fn without_a_role_a_login_waits_until_the_lead_lets_it_in() {
    let (server, fake) = server();
    let mut guest = Browser::new(&server);
    let waiting = guest.through_discord(&fake, on_server(GAST, "Gast", &[GUEST_ROLE]));
    assert_eq!(waiting.status, 403, "no role mapped: the login waits");
    assert!(text(&waiting).contains("noch nicht freigeschaltet"));
    assert!(guest.cookie("__Host-vk_portal").is_none(), "no session while waiting");

    let mut admin = Browser::new(&server);
    admin.map_roles(false);
    let overview = text(&admin.get("/admin"));
    assert!(overview.contains("Warten auf Freischaltung"), "the lead sees the waiting one");
    let csrf = admin.csrf("/admin");
    assert_eq!(admin.post("/admin/access/role", &[("csrf", &csrf), ("user", GAST), ("manual", "member")]).status, 303);
    assert_eq!(guest.through_discord(&fake, on_server(GAST, "Gast", &[GUEST_ROLE])).status, 200, "let in by hand");
    assert_eq!(guest.get("/portal/stock").status, 200);

    assert_eq!(admin.post("/admin/access/role", &[("csrf", &csrf), ("user", GAST), ("manual", "blocked")]).status, 303);
    let out = guest.get("/portal/stock");
    assert_eq!(out.status, 403, "blocking acts at once");
    assert!(headers(&out, "Set-Cookie").iter().any(|c| c.starts_with("__Host-vk_portal=;")), "the cookie is cleared");
    let blocked = guest.through_discord(&fake, on_server(GAST, "Gast", &[LEAD_ROLE]));
    assert_eq!(blocked.status, 403, "blocked beats the Discord role");
    assert!(text(&blocked).contains("gesperrt"));
}

#[test]
fn every_page_asks_for_its_role() {
    let (server, fake) = server();
    let mut lead = with_lead(&server, &fake);
    let mut logistics = Browser::new(&server);
    assert_eq!(logistics.through_discord(&fake, on_server(NOVA, "Nova", &[LOGISTICS_ROLE])).status, 200);
    let mut member = Browser::new(&server);
    assert_eq!(member.through_discord(&fake, on_server(ORION, "Orion", &[MEMBER_ROLE, GUEST_ROLE])).status, 200);

    for path in ["/portal", "/portal/stock", "/portal/blueprints", "/portal/orders", "/portal/orders/view?id=a1", "/portal/deliveries"] {
        assert_eq!(member.get(path).status, 200, "member: {path}");
    }
    for path in ["/portal/orders/new", "/portal/orders/edit?id=a1", "/portal/members", "/portal/access", "/portal/settings"] {
        assert_eq!(member.get(path).status, 403, "member: {path}");
    }
    let page = text(&member.get("/portal/orders/view?id=a1"));
    assert!(!page.contains("/portal/orders/edit") && !page.contains("/portal/orders/close"), "no buttons beyond the role");
    let tabs = text(&member.get("/portal"));
    assert!(!tabs.contains("href=\"/portal/members\"") && !tabs.contains("href=\"/portal/settings\""), "no tabs beyond the role");

    let csrf = member.csrf("/portal");
    let made = member.post("/portal/orders/new", &[("csrf", &csrf), ("item", "Laranite"), ("quantity", "5")]);
    assert_eq!(made.status, 403, "a member creates no order");
    assert_eq!(member.post("/portal/orders/close", &[("csrf", &csrf), ("id", "a1")]).status, 403);
    assert_eq!(member.post("/portal/settings", &[("csrf", &csrf), ("lead", MEMBER_ROLE)]).status, 403);
    assert_eq!(member.post("/portal/access/role", &[("csrf", &csrf), ("user", ORION), ("manual", "lead")]).status, 403);
    assert!(text(&member.get("/portal/orders?status=all")).matches("<li>").count() == 1, "still one order");

    for path in ["/portal/orders/new", "/portal/orders/edit?id=a1"] {
        assert_eq!(logistics.get(path).status, 200, "logistics: {path}");
    }
    for path in ["/portal/members", "/portal/access", "/portal/settings"] {
        assert_eq!(logistics.get(path).status, 403, "logistics: {path}");
    }
    let csrf = logistics.csrf("/portal");
    let made = logistics.post("/portal/orders/new", &[("csrf", &csrf), ("item", "Laranite"), ("quantity", "5")]);
    assert_eq!(made.status, 303);
    assert!(location(&made).starts_with("/portal/orders/view?id="), "{}", location(&made));
    assert_eq!(logistics.post("/portal/settings", &[("csrf", &csrf), ("lead", LOGISTICS_ROLE)]).status, 403);

    for path in ["/portal/members", "/portal/access", "/portal/settings", "/portal/orders/new"] {
        assert_eq!(lead.get(path).status, 200, "lead: {path}");
    }
    let access = text(&lead.get("/portal/access"));
    assert!(access.contains("Orion") && access.contains("Nova") && access.contains("Das bist du"), "{access}");
    let csrf = lead.csrf("/portal");
    let own = lead.post("/portal/access/role", &[("csrf", &csrf), ("user", VEGA), ("manual", "member")]);
    assert_eq!(own.status, 409, "the lead does not change the own role");
    assert_eq!(lead.post("/portal/access/forget", &[("csrf", &csrf), ("user", VEGA)]).status, 409);

    assert_eq!(lead.post("/portal/settings", &[("csrf", &csrf), ("lead", LEAD_ROLE), ("logistics", ""), ("member", MEMBER_ROLE)]).status, 303);
    let gone = logistics.get("/portal/orders/new");
    assert_eq!(gone.status, 403, "a changed map acts at once");
    assert!(text(&gone).contains("keinen Zugang mehr"));
    assert_eq!(logistics.get("/portal/orders").status, 303, "without a role left the session is gone");
    let bad = lead.post("/portal/settings", &[("csrf", &csrf), ("lead", "Leitung"), ("member", MEMBER_ROLE)]);
    assert_eq!(bad.status, 422);
    assert!(text(&bad).contains("keine Rollen-ID") && text(&bad).contains("value=\"Leitung\""), "the typed value stays");
}

#[test]
fn every_form_needs_the_csrf_token_of_its_session_and_the_own_origin() {
    let (server, fake) = server();
    let mut lead = with_lead(&server, &fake);
    let mut other = Browser::new(&server);
    assert_eq!(other.through_discord(&fake, on_server(ORION, "Orion", &[LEAD_ROLE])).status, 200);
    let foreign = other.csrf("/portal");
    let csrf = lead.csrf("/portal");
    let order = [("item", "Laranite"), ("quantity", "5")];
    let with = |token: &'static str| -> Vec<(&'static str, &'static str)> {
        let mut form = vec![("csrf", token)];
        form.extend_from_slice(&order);
        form
    };
    assert_eq!(lead.post("/portal/orders/new", &order).status, 403, "without a token");
    assert_eq!(lead.post("/portal/orders/new", &with("falsch")).status, 403, "a wrong token");
    let leaked: &'static str = Box::leak(foreign.into_boxed_str());
    assert_eq!(lead.post("/portal/orders/new", &with(leaked)).status, 403, "the token of another session");
    let good: &'static str = Box::leak(csrf.into_boxed_str());
    let body = urlencode(&with(good));
    assert_eq!(lead.send("POST", "/portal/orders/new", &body, &[("Origin", "https://evil.example")]).status, 403, "another origin");
    assert_eq!(lead.send("POST", "/portal/orders/new", &body, &[("Sec-Fetch-Site", "cross-site")]).status, 403, "another site");
    assert_eq!(lead.send("POST", "/portal/orders/new", &body, &[("Sec-Fetch-Site", "same-site")]).status, 403, "a neighbour of the site");
    assert!(text(&lead.get("/portal/orders?status=all")).matches("<li>").count() == 1, "no order was made");
    assert_eq!(lead.send("POST", "/portal/orders/new", &body, &[("Origin", "https://orga.example.org")]).status, 303, "the own page");
    assert_eq!(lead.get("/portal/logout").status, 405, "log-out takes no GET");
    let mut nobody = Browser::new(&server);
    assert_eq!(nobody.post("/portal/orders/new", &with(good)).status, 403, "no session, no form");
}

#[test]
fn sessions_end_after_eight_hours_after_two_unused_and_at_log_out() {
    let (server, fake) = server();
    let mut lead = with_lead(&server, &fake);
    for _ in 0..5 {
        lead.now += 90 * 60;
        assert_eq!(lead.get("/portal/stock").status, 200, "in use, under eight hours");
    }
    lead.now += 31 * 60;
    let late = lead.get("/portal/stock");
    assert_eq!(late.status, 303, "after eight hours the session ends however busy");
    assert_eq!(location(&late), "/portal");

    let mut lead = Browser::new(&server);
    assert_eq!(lead.through_discord(&fake, on_server(VEGA, "Vega", &[LEAD_ROLE])).status, 200);
    lead.now += portal::IDLE_SECONDS - 60;
    assert_eq!(lead.get("/portal").status, 200);
    lead.now += portal::IDLE_SECONDS;
    assert_eq!(lead.get("/portal/stock").status, 303, "two hours unused end the session");

    let mut lead = Browser::new(&server);
    assert_eq!(lead.through_discord(&fake, on_server(VEGA, "Vega", &[LEAD_ROLE])).status, 200);
    let kept = lead.cookie("__Host-vk_portal").unwrap();
    let csrf = lead.csrf("/portal");
    let out = lead.post("/portal/logout", &[("csrf", &csrf)]);
    assert_eq!((out.status, location(&out).as_str()), (303, "/portal?out=1"));
    assert!(lead.cookie("__Host-vk_portal").is_none(), "the cookie is cleared");
    lead.cookies.push(("__Host-vk_portal".into(), kept));
    assert_eq!(lead.get("/portal/stock").status, 303, "the old cookie no longer counts");
    assert!(text(&lead.get("/portal?out=1")).contains("Abgemeldet"));
}

#[test]
fn logistics_confirms_deliveries_and_a_member_cannot() {
    let (server, fake) = server();
    let _lead = with_lead(&server, &fake);
    let body = urlencode(&[("scope", "orders:write")]);
    let code = server.handle_at(&Request::new("POST", "/versekit/v1/device/code", &[("X-Forwarded-Proto", "https")], body.as_bytes()), T0).json_body();
    assert!(server.approve(code["user_code"].as_str().unwrap(), "vega", None));
    let form = urlencode(&[("grant_type", "urn:ietf:params:oauth:grant-type:device_code"), ("device_code", code["device_code"].as_str().unwrap())]);
    let tokens = server.handle_at(&Request::new("POST", "/versekit/v1/token", &[("X-Forwarded-Proto", "https")], form.as_bytes()), T0).json_body();
    let bearer = format!("Bearer {}", tokens["access_token"].as_str().unwrap());
    let booked = server.handle_at(
        &Request::new(
            "POST",
            "/versekit/v1/orders/a1/deliveries",
            &[("Authorization", bearer.as_str()), ("Idempotency-Key", "k1"), ("X-Forwarded-Proto", "https")],
            br#"{"items":[{"item":"Quantanium","quantity":4}]}"#,
        ),
        T0,
    );
    assert_eq!(booked.status, 201);
    let id = booked.json_body()["delivery_id"].as_str().unwrap().to_string();

    let mut member = Browser::new(&server);
    assert_eq!(member.through_discord(&fake, on_server(ORION, "Orion", &[MEMBER_ROLE])).status, 200);
    let list = text(&member.get("/portal/deliveries"));
    assert!(list.contains("Unbestätigt") && !list.contains("/portal/deliveries/confirm"), "a member sees, but has no button");
    let csrf = member.csrf("/portal");
    assert_eq!(member.post("/portal/deliveries/confirm", &[("csrf", &csrf), ("id", &id)]).status, 403);

    let mut logistics = Browser::new(&server);
    assert_eq!(logistics.through_discord(&fake, on_server(NOVA, "Nova", &[LOGISTICS_ROLE])).status, 200);
    assert!(text(&logistics.get("/portal/orders/view?id=a1")).contains("/portal/deliveries/confirm"));
    let csrf = logistics.csrf("/portal");
    let done = logistics.post("/portal/deliveries/confirm", &[("csrf", &csrf), ("id", &id), ("back", "order")]);
    assert_eq!((done.status, location(&done).as_str()), (303, "/portal/orders/view?id=a1&bestaetigt=1"));
    assert!(text(&member.get("/portal/deliveries")).contains("Bestätigt von Nova"));
    assert_eq!(logistics.post("/portal/deliveries/confirm", &[("csrf", &csrf), ("id", "gibt-es-nicht")]).status, 404);
    let orders = server.handle_at(&Request::new("GET", "/versekit/v1/orders", &[("Authorization", bearer.as_str()), ("X-Forwarded-Proto", "https")], b""), T0);
    assert_eq!(orders.status, 403, "the token has orders:write only");
    let api = serde_json::to_string(&server.handle_at(&Request::new("GET", "/.well-known/versekit-org.json", &[("X-Forwarded-Proto", "https")], b""), T0).json_body()).unwrap();
    assert!(!api.contains("portal"), "the standard's discovery stays as it was");
}

#[test]
fn people_and_roles_stay_across_a_restart_sessions_and_secrets_do_not() {
    let dir = std::env::temp_dir().join(format!("versekit-orgserver-portal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("state.json");
    let fake = FakeDiscord::new();
    let mut with_file = config();
    with_file.state_file = Some(file.clone());
    let server = Server::new(with_file.clone()).with_outbound(fake.clone());
    let mut lead = with_lead(&server, &fake);
    let csrf = lead.csrf("/portal");
    let kept = lead.cookie("__Host-vk_portal").unwrap();
    let mut guest = Browser::new(&server);
    let _ = guest.through_discord(&fake, on_server(GAST, "Gast", &[GUEST_ROLE]));
    assert_eq!(lead.post("/portal/access/role", &[("csrf", &csrf), ("user", GAST), ("manual", "blocked")]).status, 303);
    drop(server);

    let saved = std::fs::read_to_string(&file).unwrap();
    let root: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(root["format"], 3);
    assert_eq!(root["portal"]["users"][VEGA]["name"], "Vega");
    assert_eq!(root["portal"]["users"][GAST]["manual"], "blocked");
    assert_eq!(root["portal"]["roles"]["lead"][0], LEAD_ROLE);
    for secret in [SECRET, "discord-zugang-123", "abcDEF123", kept.as_str(), csrf.as_str()] {
        assert!(!saved.contains(secret), "the state file holds no {secret}");
    }

    let again = Server::new(with_file).with_outbound(fake.clone());
    let mut back = Browser::new(&again);
    back.cookies.push(("__Host-vk_portal".into(), kept));
    assert_eq!(back.get("/portal/stock").status, 303, "sessions live in memory only");
    let blocked = Browser::new(&again).through_discord(&fake, on_server(GAST, "Gast", &[MEMBER_ROLE]));
    assert_eq!(blocked.status, 403, "the block stays");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_test_mode_drops_the_host_prefix_and_secure_and_limits_hold() {
    let fake = FakeDiscord::new();
    let plain = Config::new("http://127.0.0.1:8110", "Nebelorga", "NBL").with_discord(CLIENT, SECRET, GUILD).unwrap();
    let server = Server::new(plain).with_outbound(fake.clone());
    let answer = server.handle_at(&Request::new("GET", "/portal/login", &[], b""), T0);
    let cookie = headers(&answer, "Set-Cookie").join(" ");
    assert!(cookie.starts_with("vk_portal_login=") && !cookie.contains("Secure"), "{cookie}");
    assert!(location(&answer).contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A8110%2Fportal%2Fcallback"));

    let (server, _) = self::server();
    let mut browser = Browser::new(&server);
    for _ in 0..portal::LOGINS_MAX {
        assert_eq!(browser.get("/portal/login").status, 303);
    }
    let full = browser.get("/portal/login");
    assert_eq!(full.status, 429, "too many logins waiting");
    assert!(headers(&full, "Retry-After").contains(&"60"));
    browser.now += portal::LOGIN_SECONDS + 1;
    assert_eq!(browser.get("/portal/login").status, 303, "expired logins make room");
}

#[test]
fn the_admin_door_keeps_working_and_shows_access_and_settings() {
    let (server, fake) = server();
    let mut admin = Browser::new(&server);
    admin.map_roles(false);
    let overview = text(&admin.get("/admin"));
    assert!(overview.contains("Notzugang") && overview.contains("href=\"/admin/access\"") && overview.contains("href=\"/admin/settings\""));
    let settings = text(&admin.get("/admin/settings"));
    assert!(settings.contains("https://orga.example.org/portal/callback") && settings.contains(GUILD) && !settings.contains(SECRET));
    assert!(loads_nothing_from_elsewhere(&settings));
    let _ = Browser::new(&server).through_discord(&fake, on_server(ORION, "Orion", &[MEMBER_ROLE]));
    let csrf = admin.csrf("/admin");
    let blocked = admin.post("/admin/access/role", &[("csrf", &csrf), ("user", ORION), ("manual", "blocked")]);
    assert_eq!(blocked.status, 303, "the admin password may set every role");
    let forgot = admin.post("/admin/access/forget", &[("csrf", &csrf), ("user", ORION)]);
    assert_eq!(forgot.status, 303);
    assert!(!text(&admin.get("/admin/access")).contains(ORION));
    assert_eq!(admin.post("/admin/access/role", &[("csrf", &csrf), ("user", ORION), ("manual", "lead")]).status, 404, "nobody to set");
    let _ = Browser::new(&server).through_discord(&fake, on_server(NOVA, "Nova", &[MEMBER_ROLE]));
    assert_eq!(admin.post("/admin/access/role", &[("csrf", &csrf), ("user", NOVA), ("manual", "kaiser")]).status, 422, "no such role");
}
