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

//! The administration at `/admin`: off without a password; nobody in without it; the login brakes; every form needs
//! the session's CSRF token and the own origin; every page answers in German and English and loads nothing from
//! elsewhere; orders are created, edited, closed and show their deliveries; wrong input changes nothing; members show
//! their rights and last contact and can be disconnected; stock and blueprints are listed and searched; the status
//! page shows nothing of it.

use serde_json::{Value, json};
use versekit_orgserver::http::{Request, Response};
use versekit_orgserver::{Config, Server};

const PASSWORD: &str = "orga-leitung-passwort-42";
const VEGA_KEY: &str = "schluessel-von-vega-0123";
const NOVA_KEY: &str = "schluessel-von-nova-4567";
/// 2027-01-15 08:00 UTC.
const T0: u64 = 1_800_000_000;

fn config() -> Config {
    Config::new("https://orga.example.org", "Nebelorga", "NBL")
        .with_member("vega", "Vega", VEGA_KEY)
        .with_member("nova", "Nova", NOVA_KEY)
        .with_order(json!({"id": "a1", "title": "Erz für die Werft", "items": [{"item": "Quantanium", "quantity": 10, "unit": "scu"}]}))
}

fn server() -> Server {
    Server::new(config().with_admin_password(PASSWORD).unwrap())
}

fn text(answer: &Response) -> String {
    String::from_utf8(answer.body.clone()).unwrap()
}

fn headers<'a>(answer: &'a Response, name: &str) -> Vec<&'a str> {
    answer.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).collect()
}

fn location(answer: &Response) -> &str {
    headers(answer, "Location").first().copied().unwrap_or_default()
}

fn urlencode(pairs: &[(&str, &str)]) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect::<String>()
    };
    pairs.iter().map(|(n, v)| format!("{}={}", enc(n), enc(v))).collect::<Vec<_>>().join("&")
}

/// The value of the hidden field `name` in a page.
fn hidden(page: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let start = page.find(&marker).unwrap_or_else(|| panic!("no field {name} in {page}")) + marker.len();
    page[start..].split('"').next().unwrap().to_string()
}

/// Nothing from another server: no script, no stylesheet, font or picture from elsewhere.
fn loads_nothing_from_elsewhere(page: &str) -> bool {
    let own = page.replacen("<link rel=\"stylesheet\" href=\"/assets/look.css?v=", "", 1);
    !["<script", "src=\"http", "src='http", "url(http", "url(//", "@import", "<link ", "<style", " style="].iter().any(|p| own.contains(p))
}

/// A browser with its cookies, its language and the clock of the server.
struct Browser<'a> {
    server: &'a Server,
    cookies: Vec<(String, String)>,
    language: &'static str,
    now: u64,
}

impl<'a> Browser<'a> {
    fn new(server: &'a Server) -> Browser<'a> {
        Browser { server, cookies: Vec::new(), language: "de-DE,de;q=0.9", now: T0 }
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
        list.extend_from_slice(extra);
        let answer = self.server.handle_at(&Request::new(method, target, &list, body.as_bytes()), self.now);
        for value in headers(&answer, "Set-Cookie") {
            let pair = value.split(';').next().unwrap();
            let (name, content) = pair.split_once('=').unwrap();
            self.cookies.retain(|(n, _)| n != name);
            if !content.is_empty() {
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

    fn log_in(&mut self, password: &str) -> Response {
        let form = text(&self.get("/admin/login"));
        let token = hidden(&form, "csrf");
        self.post("/admin/login", &[("csrf", token.as_str()), ("password", password)])
    }

    fn csrf(&mut self) -> String {
        hidden(&text(&self.get("/admin")), "csrf")
    }

    fn session(&self) -> Option<&str> {
        self.cookies.iter().find(|(n, _)| n == "vk_admin").map(|(_, v)| v.as_str())
    }
}

/// A member logged in through the API at the time `now`, with the rights asked for.
struct Member {
    access: String,
    refresh: String,
}

fn member(server: &Server, id: &str, scopes: &str, now: u64) -> Member {
    let body = urlencode(&[("scope", scopes)]);
    let code = server.handle_at(&Request::new("POST", "/versekit/v1/device/code", &[], body.as_bytes()), now).json_body();
    assert!(server.approve(code["user_code"].as_str().unwrap(), id, None));
    let form = urlencode(&[
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("device_code", code["device_code"].as_str().unwrap()),
    ]);
    let tokens = server.handle_at(&Request::new("POST", "/versekit/v1/token", &[], form.as_bytes()), now).json_body();
    Member {
        access: tokens["access_token"].as_str().expect("a token").to_string(),
        refresh: tokens["refresh_token"].as_str().expect("a token").to_string(),
    }
}

fn api(server: &Server, who: &Member, method: &str, path: &str, body: &str, now: u64) -> Response {
    let bearer = format!("Bearer {}", who.access);
    let mut list = vec![("Authorization", bearer.as_str())];
    if method == "POST" {
        list.push(("Idempotency-Key", "lieferung-1"));
    }
    server.handle_at(&Request::new(method, &format!("/versekit/v1{path}"), &list, body.as_bytes()), now)
}

fn api_orders(server: &Server, who: &Member, now: u64) -> Vec<Value> {
    api(server, who, "GET", "/orders", "", now).json_body()["orders"].as_array().cloned().unwrap_or_default()
}

const PAGES: [&str; 10] = [
    "/admin",
    "/admin/members",
    "/admin/stock",
    "/admin/blueprints",
    "/admin/orders",
    "/admin/orders?status=all",
    "/admin/orders/new",
    "/admin/orders/view?id=a1",
    "/admin/orders/edit?id=a1",
    "/admin/deliveries",
];

#[test]
fn without_a_password_the_administration_is_off_and_says_so() {
    let server = Server::new(config());
    let mut browser = Browser::new(&server);
    for path in ["/admin", "/admin/login", "/admin/orders", "/admin/members"] {
        let answer = browser.get(path);
        assert_eq!(answer.status, 404, "{path}");
        let page = text(&answer);
        assert!(page.contains("Die Verwaltung ist aus.") && page.contains("VERSEKIT_ORG_ADMIN_PASSWORD"), "{path}: {page}");
    }
    let answer = browser.post("/admin/login", &[("csrf", "x"), ("password", PASSWORD)]);
    assert_eq!(answer.status, 404);
    assert!(browser.session().is_none(), "no session without a password");

    assert!(config().with_admin_password("zu-kurz").unwrap_err().contains("shorter"));
    let set = config()
        .with_env(|name| (name == "VERSEKIT_ORG_ADMIN_PASSWORD").then(|| PASSWORD.to_string()))
        .unwrap();
    assert!(!format!("{set:?}").contains(PASSWORD), "the debug output holds the password");
    let on = Server::new(set);
    assert_eq!(Browser::new(&on).get("/admin").status, 303, "the environment turns the administration on");
    let blank = config().with_env(|name| (name == "VERSEKIT_ORG_ADMIN_PASSWORD").then(String::new)).unwrap();
    assert_eq!(Browser::new(&Server::new(blank)).get("/admin").status, 404, "an empty variable leaves it off");
}

#[test]
fn without_the_password_nobody_gets_in() {
    let server = server();
    let mut stranger = Browser::new(&server);
    for path in PAGES {
        let answer = stranger.get(path);
        assert_eq!((answer.status, location(&answer)), (303, "/admin/login"), "{path}");
    }
    stranger.cookies.push(("vk_admin".into(), "a".repeat(64)));
    assert_eq!(stranger.get("/admin").status, 303, "a made-up session");
    let posted = stranger.post("/admin/orders/new", &[("csrf", "x"), ("item", "Gold"), ("quantity", "1")]);
    assert_eq!(posted.status, 403);

    let mut browser = Browser::new(&server);
    let wrong = browser.log_in("falsches-passwort-123");
    assert_eq!(wrong.status, 401);
    assert!(text(&wrong).contains("Das Passwort stimmt nicht."));
    assert!(browser.session().is_none());

    let right = browser.log_in(PASSWORD);
    assert_eq!((right.status, location(&right)), (303, "/admin"));
    let cookie = headers(&right, "Set-Cookie").into_iter().find(|c| c.starts_with("vk_admin=")).unwrap();
    for part in ["HttpOnly", "SameSite=Strict", "Path=/admin", "Secure", "Max-Age=28800"] {
        assert!(cookie.contains(part), "{cookie} lacks {part}");
    }
    for path in PAGES {
        assert_eq!(browser.get(path).status, 200, "{path}");
    }
    let old = browser.session().unwrap().to_string();
    let csrf = browser.csrf();
    let out = browser.post("/admin/logout", &[("csrf", csrf.as_str())]);
    assert_eq!((out.status, location(&out)), (303, "/admin/login?out=1"));
    assert!(text(&browser.get("/admin/login?out=1")).contains("Abgemeldet."));
    browser.cookies.push(("vk_admin".into(), old));
    assert_eq!(browser.get("/admin").status, 303, "the session ends with the log-out");

    let plain = Server::new(
        Config::new("http://127.0.0.1:8110", "Testorga", "TST").with_admin_password(PASSWORD).unwrap(),
    );
    let answer = Browser::new(&plain).log_in(PASSWORD);
    let cookie = headers(&answer, "Set-Cookie").into_iter().find(|c| c.starts_with("vk_admin=")).unwrap();
    assert!(!cookie.contains("Secure"), "no Secure on plain http of the own computer");

    let mut late = Browser::new(&server);
    late.log_in(PASSWORD);
    late.now = T0 + 8 * 3600 + 1;
    assert_eq!(late.get("/admin").status, 303, "a session ends after eight hours");
}

#[test]
fn the_login_brakes_after_five_wrong_passwords() {
    let server = server();
    let mut browser = Browser::new(&server);
    for round in 1..=4 {
        assert_eq!(browser.log_in("falsch-falsch-falsch").status, 401, "try {round}");
    }
    let fifth = browser.log_in("falsch-falsch-falsch");
    assert_eq!(fifth.status, 429);
    assert!(text(&fifth).contains("Zu viele falsche Passwörter"));
    assert_eq!(headers(&fifth, "Retry-After"), ["60"]);

    browser.now = T0 + 30;
    assert_eq!(browser.log_in(PASSWORD).status, 429, "even the right password waits");
    assert!(browser.session().is_none());
    browser.now = T0 + 61;
    assert_eq!(browser.log_in("falsch-falsch-falsch").status, 429, "the sixth wrong one waits longer");
    browser.now = T0 + 61 + 119;
    assert_eq!(browser.log_in(PASSWORD).status, 429);
    browser.now = T0 + 61 + 121;
    assert_eq!(browser.log_in(PASSWORD).status, 303);
    assert!(browser.session().is_some());
    let mut other = Browser::new(&server);
    other.now = browser.now;
    assert_eq!(other.log_in("falsch-falsch-falsch").status, 401, "a good login starts the count anew");
}

#[test]
fn a_form_without_the_right_csrf_token_or_from_elsewhere_is_refused() {
    let server = server();
    let mut browser = Browser::new(&server);
    browser.log_in(PASSWORD);
    let csrf = browser.csrf();
    let order = [("item", "Gold"), ("quantity", "3"), ("unit", "scu")];
    let count = |cookies: &[(String, String)]| {
        text(&Browser { cookies: cookies.to_vec(), ..Browser::new(&server) }.get("/admin/orders?status=all"))
    };
    let before = count(&browser.cookies);

    let wrong = browser.post("/admin/orders/new", &[&[("csrf", "0123456789abcdef0123456789abcdef")][..], &order[..]].concat());
    assert_eq!(wrong.status, 403);
    assert!(text(&wrong).contains("Das Formular ist abgelaufen"));
    assert_eq!(browser.post("/admin/orders/new", &order).status, 403, "no token at all");
    let fields = urlencode(&[&[("csrf", csrf.as_str())][..], &order[..]].concat());
    let foreign = browser.send("POST", "/admin/orders/new", &fields, &[("Origin", "https://boese.example")]);
    assert_eq!(foreign.status, 403, "another origin");
    let site = browser.send("POST", "/admin/orders/new", &fields, &[("Sec-Fetch-Site", "cross-site")]);
    assert_eq!(site.status, 403, "another site");
    assert_eq!(count(&browser.cookies), before, "no refused form created an order");
    let mut members = Browser { cookies: browser.cookies.clone(), ..Browser::new(&server) };
    assert_eq!(members.post("/admin/members/disconnect", &[("csrf", "x"), ("member", "vega")]).status, 403);
    assert_eq!(members.post("/admin/logout", &[("csrf", "x")]).status, 403);

    let own = browser.send("POST", "/admin/orders/new", &fields, &[("Origin", "https://orga.example.org")]);
    assert_eq!(own.status, 303, "the own form with the token");
    assert_ne!(count(&browser.cookies), before);

    let mut stranger = Browser::new(&server);
    let token = hidden(&text(&Browser::new(&server).get("/admin/login")), "csrf");
    let login = stranger.post("/admin/login", &[("csrf", token.as_str()), ("password", PASSWORD)]);
    assert_eq!(login.status, 200, "a login form of another browser gets a fresh form");
    assert!(stranger.session().is_none(), "but no session");
    let mut tied = Browser::new(&server);
    let token = hidden(&text(&tied.get("/admin/login")), "csrf");
    let login = tied.send(
        "POST",
        "/admin/login",
        &urlencode(&[("csrf", token.as_str()), ("password", PASSWORD)]),
        &[("Origin", "https://boese.example")],
    );
    assert_eq!(login.status, 403, "a login from another origin");
}

#[test]
fn an_expired_login_form_gives_a_fresh_one_and_password_managers_find_their_field() {
    let server = server();
    let mut browser = Browser::new(&server);
    let page = text(&browser.get("/admin/login"));
    assert!(page.contains("Das Formular gilt 10 Minuten"), "the form says how long it lasts");
    assert!(
        page.contains("<input type=\"text\" name=\"username\" value=\"admin\" autocomplete=\"username\" hidden>"),
        "a password-only form names its user for password managers"
    );
    assert!(page.contains("autocomplete=\"current-password\" required>"));
    let old = hidden(&page, "csrf");
    // After its Max-Age of 600 seconds the browser drops the cookie of the form.
    browser.now += 601;
    browser.cookies.retain(|(n, _)| n != "vk_admin_login");
    let late = browser.post("/admin/login", &[("csrf", old.as_str()), ("password", PASSWORD)]);
    let fresh = text(&late);
    assert_eq!(late.status, 200, "an expired form is no error");
    assert!(fresh.contains("Hier ist eine frische") && !fresh.contains("abgelaufen oder kam nicht"), "{fresh}");
    assert!(browser.session().is_none(), "the expired form lets nobody in, even with the right password");
    let new = hidden(&fresh, "csrf");
    assert_ne!(new, old, "the fresh form carries a new token");
    assert!(browser.cookies.iter().any(|(n, v)| n == "vk_admin_login" && *v == new), "and ties it to this browser again");
    assert_eq!(browser.post("/admin/login", &[("csrf", new.as_str()), ("password", PASSWORD)]).status, 303, "the fresh form logs in");
    assert!(browser.session().is_some());

    let mut other = Browser::new(&server);
    let token = hidden(&text(&other.get("/admin/login")), "csrf");
    let body = urlencode(&[("csrf", token.as_str()), ("password", PASSWORD)]);
    let foreign = other.send("POST", "/admin/login", &body, &[("Origin", "https://boese.example")]);
    assert_eq!(foreign.status, 403, "a form from another origin stays refused");
    assert!(text(&foreign).contains("kam nicht von dieser Seite") && other.session().is_none());
}

#[test]
fn a_browser_that_holds_the_form_cookie_back_logs_in_from_the_own_page_only() {
    let server = server();
    let form = |browser: &mut Browser| {
        let token = hidden(&text(&browser.get("/admin/login")), "csrf");
        browser.cookies.retain(|(n, _)| n != "vk_admin_login");
        urlencode(&[("csrf", token.as_str()), ("password", PASSWORD)])
    };
    let mut fetch = Browser::new(&server);
    let body = form(&mut fetch);
    assert_eq!(fetch.send("POST", "/admin/login", &body, &[("Sec-Fetch-Site", "same-origin")]).status, 303, "Sec-Fetch-Site vouches");
    assert!(fetch.session().is_some());
    let mut origin = Browser::new(&server);
    let body = form(&mut origin);
    assert_eq!(origin.send("POST", "/admin/login", &body, &[("Origin", "https://orga.example.org")]).status, 303, "Origin vouches");

    let mut silent = Browser::new(&server);
    let body = form(&mut silent);
    let answer = silent.send("POST", "/admin/login", &body, &[]);
    assert_eq!(answer.status, 200, "no cookie and no word of the browser: a fresh form");
    assert!(silent.session().is_none() && text(&answer).contains("Hier ist eine frische"));
    let again = silent.send("POST", "/admin/login", &body, &[("Sec-Fetch-Site", "same-origin")]);
    assert_eq!(again.status, 200, "a token counts once");
    assert!(silent.session().is_none());

    let mut tied = Browser::new(&server);
    let token = hidden(&text(&tied.get("/admin/login")), "csrf");
    let body = urlencode(&[("csrf", token.as_str()), ("password", PASSWORD)]);
    assert_eq!(tied.send("POST", "/admin/login", &body, &[]).status, 303, "the cookie ties as before");
    let mut copy = Browser::new(&server);
    assert_eq!(copy.send("POST", "/admin/login", &body, &[("Sec-Fetch-Site", "same-origin")]).status, 200, "a used token does not count again");
    assert!(copy.session().is_none());

    let mut late = Browser::new(&server);
    let body = form(&mut late);
    late.now += 601;
    assert_eq!(late.send("POST", "/admin/login", &body, &[("Sec-Fetch-Site", "same-origin")]).status, 200, "older than 10 minutes");
    assert!(late.session().is_none());

    let mut foreign = Browser::new(&server);
    let body = form(&mut foreign);
    for headers in [
        vec![("Sec-Fetch-Site", "cross-site")],
        vec![("Sec-Fetch-Site", "same-site")],
        vec![("Sec-Fetch-Site", "same-origin"), ("Origin", "https://boese.example")],
        vec![("Origin", "null"), ("Sec-Fetch-Site", "cross-site")],
    ] {
        let answer = foreign.send("POST", "/admin/login", &body, &headers);
        assert_eq!(answer.status, 403, "{headers:?}");
        assert!(foreign.session().is_none());
    }
    let refused = text(&foreign.send("POST", "/admin/login", &body, &[("Origin", "https://boese.example")]));
    assert!(refused.contains("Herkunft https://boese.example, Adresse orga.example.org"), "the page names both");

    let mut wrong = Browser::new(&server);
    let body = form(&mut wrong).replace(PASSWORD, "falsches-passwort-123");
    assert_eq!(wrong.send("POST", "/admin/login", &body, &[("Sec-Fetch-Site", "same-origin")]).status, 401, "the password is checked as ever");
}

/// A browser that sends `Origin: null` with a form (Chromium does so with `Referrer-Policy: no-referrer`).
#[test]
fn a_form_with_origin_null_logs_in_with_its_token_and_only_then() {
    let server = server();
    let null = [("Origin", "null")];
    let fresh = |browser: &mut Browser, hold_cookie_back: bool| {
        let token = hidden(&text(&browser.get("/admin/login")), "csrf");
        if hold_cookie_back {
            browser.cookies.retain(|(n, _)| n != "vk_admin_login");
        }
        urlencode(&[("csrf", token.as_str()), ("password", PASSWORD)])
    };

    let mut tied = Browser::new(&server);
    let body = fresh(&mut tied, false);
    assert_eq!(tied.send("POST", "/admin/login", &body, &null).status, 303, "the cookie of the form and the token");
    assert!(tied.session().is_some());

    let mut held_back = Browser::new(&server);
    let body = fresh(&mut held_back, true);
    assert_eq!(held_back.send("POST", "/admin/login", &body, &null).status, 303, "a token of this server alone");
    assert!(held_back.session().is_some());
    assert_eq!(held_back.send("POST", "/admin/login", &body, &null).status, 403, "a token counts once");

    let mut unknown = Browser::new(&server);
    let body = urlencode(&[("csrf", "0123456789abcdef0123456789abcdef"), ("password", PASSWORD)]);
    assert_eq!(unknown.send("POST", "/admin/login", &body, &null).status, 403, "a token nobody handed out");
    assert!(unknown.session().is_none());
    let mut none = Browser::new(&server);
    let body = urlencode(&[("password", PASSWORD)]);
    assert_eq!(none.send("POST", "/admin/login", &body, &null).status, 403, "no token at all");

    for site in ["cross-site", "same-site"] {
        let mut other = Browser::new(&server);
        let body = fresh(&mut other, false);
        let answer = other.send("POST", "/admin/login", &body, &[("Origin", "null"), ("Sec-Fetch-Site", site)]);
        assert_eq!(answer.status, 403, "null from {site}");
        assert!(other.session().is_none());
    }

    let mut wrong = Browser::new(&server);
    let body = fresh(&mut wrong, false).replace(PASSWORD, "falsches-passwort-123");
    assert_eq!(wrong.send("POST", "/admin/login", &body, &null).status, 401, "the password is checked as ever");
    assert!(wrong.session().is_none());
}

#[test]
fn the_forms_of_the_administration_with_origin_null_work_with_the_csrf_token() {
    let server = server();
    let mut browser = Browser::new(&server);
    browser.log_in(PASSWORD);
    let csrf = browser.csrf();
    let order = [("item", "Gold"), ("quantity", "3"), ("unit", "scu")];
    let with = |token: &str| urlencode(&[&[("csrf", token)][..], &order[..]].concat());
    let null = [("Origin", "null")];
    assert_eq!(browser.send("POST", "/admin/orders/new", &with(&csrf), &null).status, 303, "own token and null");
    assert_eq!(
        browser.send("POST", "/admin/orders/new", &with("0123456789abcdef0123456789abcdef"), &null).status,
        403,
        "a wrong token and null"
    );
    let from_site = [("Origin", "null"), ("Sec-Fetch-Site", "cross-site")];
    assert_eq!(browser.send("POST", "/admin/orders/new", &with(&csrf), &from_site).status, 403, "null from another site");
    let foreign = [("Origin", "https://boese.example")];
    assert_eq!(browser.send("POST", "/admin/orders/new", &with(&csrf), &foreign).status, 403, "another origin stays refused");
}

/// Firefox on an http address in the home network: no `Sec-Fetch-*` (only sent to https and localhost), no `Referer`,
/// `Origin: null` after the policy `no-referrer`.
#[test]
fn firefox_on_an_http_address_of_the_home_network_logs_in() {
    let server = Server::new(
        Config::new("http://192.0.2.76:8110", "Nebelorga", "NBL").with_admin_password(PASSWORD).unwrap(),
    );
    let firefox = |method: &str, target: &str, extra: &[(&str, &str)], body: &str| {
        let mut list = vec![
            ("Host", "192.0.2.76:8110"),
            ("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:133.0) Gecko/20100101 Firefox/133.0"),
            ("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
            ("Accept-Language", "de,en-US;q=0.7,en;q=0.3"),
        ];
        if method == "POST" {
            list.push(("Content-Type", "application/x-www-form-urlencoded"));
        }
        list.extend_from_slice(extra);
        server.handle_at(&Request::new(method, target, &list, body.as_bytes()), T0)
    };
    let page = firefox("GET", "/admin/login", &[], "");
    assert_eq!(headers(&page, "Referrer-Policy"), ["same-origin"], "the own origin is named, no other");
    let set = headers(&page, "Set-Cookie").first().copied().unwrap_or_default().to_string();
    assert!(set.contains("SameSite=Strict") && !set.contains("Secure"), "an http address gets no Secure cookie: {set}");
    let token = hidden(&text(&page), "csrf");
    let cookie = format!("vk_admin_login={token}");
    let body = urlencode(&[("csrf", token.as_str()), ("password", PASSWORD)]);
    let with_cookie = firefox("POST", "/admin/login", &[("Origin", "null"), ("Cookie", cookie.as_str())], &body);
    assert_eq!(with_cookie.status, 303, "Origin null, the cookie, no Sec-Fetch-Site");

    let page = firefox("GET", "/admin/login", &[], "");
    let token = hidden(&text(&page), "csrf");
    let body = urlencode(&[("csrf", token.as_str()), ("password", PASSWORD)]);
    let without = firefox("POST", "/admin/login", &[("Origin", "null")], &body);
    assert_eq!(without.status, 303, "Origin null, the cookie held back by the tracking protection");
}

#[test]
fn the_password_field_does_not_depend_on_the_web_font_for_its_dots() {
    let server = server();
    let sheet = text(&Browser::new(&server).get("/assets/look.css"));
    assert!(
        sheet.contains("input[type=password]{font-family:system-ui,\"Segoe UI\",Arial,sans-serif;"),
        "the dots of a password field come from a system font"
    );
}

#[test]
fn typed_text_shows_in_every_field_also_when_a_password_manager_fills_it() {
    let server = server();
    let sheet = text(&Browser::new(&server).get("/assets/look.css"));
    for rule in [
        "input,select,textarea{color-scheme:dark;caret-color:var(--neon);-webkit-text-fill-color:var(--text)}",
        "input::placeholder,textarea::placeholder{-webkit-text-fill-color:var(--leise)}",
        "input:-webkit-autofill,input:-webkit-autofill:hover,input:-webkit-autofill:focus",
        "-webkit-box-shadow:0 0 0 1000px var(--grund) inset",
        "input:autofill{-webkit-text-fill-color:var(--text)",
    ] {
        assert!(sheet.contains(rule), "{rule}");
    }
}

#[test]
fn every_page_answers_in_both_languages_and_loads_nothing_from_elsewhere() {
    let server = server();
    let mut browser = Browser::new(&server);
    browser.log_in(PASSWORD);
    for path in PAGES {
        browser.language = "de-DE,de;q=0.9";
        let german = browser.get(path);
        browser.language = "en-GB,en;q=0.9";
        let english = browser.get(path);
        for answer in [&german, &english] {
            assert_eq!(answer.status, 200, "{path}");
            assert!(headers(answer, "Content-Security-Policy")[0].contains("default-src 'none'"), "{path}");
            assert!(loads_nothing_from_elsewhere(&text(answer)), "{path}");
            assert!(text(answer).contains("Nebelorga"), "{path}: the head names the org");
        }
        assert!(text(&german).contains("lang=\"de\"") && text(&german).contains("Abmelden"), "{path}");
        assert!(text(&english).contains("lang=\"en\"") && text(&english).contains("Log out"), "{path}");
    }
    browser.language = "de";
    assert!(text(&browser.get("/admin/stock")).contains("href=\"/admin/stock\" aria-current=\"page\""));
    let missing = browser.get("/admin/orders/view?id=gibt-es-nicht");
    assert_eq!(missing.status, 404);
    assert_eq!(browser.get("/admin/nirgends").status, 404);
    assert_eq!(browser.send("PUT", "/admin/orders", "", &[]).status, 405);
}

#[test]
fn orders_are_created_edited_closed_and_show_their_deliveries() {
    let server = server();
    let mut browser = Browser::new(&server);
    browser.log_in(PASSWORD);
    let csrf = browser.csrf();
    let created = browser.post(
        "/admin/orders/new",
        &[
            ("csrf", csrf.as_str()),
            ("title", ""),
            ("item", "Laranite"),
            ("quantity", "2,5"),
            ("unit", "scu"),
            ("item", "Kühlmodul Bracer"),
            ("quantity", "4"),
            ("unit", ""),
            ("item", ""),
            ("quantity", ""),
            ("unit", ""),
            ("location", "Area18, Hangar 3"),
            ("due", "2027-02-01"),
            ("note", "Bitte vor dem\nStaffelabend"),
        ],
    );
    assert_eq!(created.status, 303);
    let view = location(&created).to_string();
    assert!(view.starts_with("/admin/orders/view?id=a-") && view.ends_with("&gespeichert=1"), "{view}");
    let id = view.trim_start_matches("/admin/orders/view?id=").trim_end_matches("&gespeichert=1").to_string();
    let page = text(&browser.get(&view));
    assert!(page.contains("Gespeichert.") && page.contains("<h1>Laranite</h1>"), "the title is the first good: {page}");
    assert!(page.contains("0 / 2,5 SCU") && page.contains("0 / 4 Stk.") && page.contains("Area18, Hangar 3"));
    assert!(page.contains("Noch keine Lieferung."));

    let vega = member(&server, "vega", "orders:read orders:write", T0);
    let listed = api_orders(&server, &vega, T0);
    let order = listed.iter().find(|o| o["id"] == id.as_str()).expect("the new order in the API");
    assert_eq!(order["status"], "open");
    assert_eq!((order["location"].as_str(), order["due"].as_str()), (Some("Area18, Hangar 3"), Some("2027-02-01")));
    assert_eq!(order["note"], "Bitte vor dem\nStaffelabend");
    assert_eq!(order["items"][0], json!({"item": "Laranite", "quantity": 2.5, "unit": "scu", "delivered": 0.0}));

    let delivered = api(&server, &vega, "POST", &format!("/orders/{id}/deliveries"), r#"{"items":[{"item":"Kühlmodul Bracer","quantity":3}]}"#, T0 + 60);
    assert_eq!(delivered.status, 201);
    let page = text(&browser.get(&format!("/admin/orders/view?id={id}")));
    assert!(page.contains("3 / 4 Stk.") && page.contains("3 Kühlmodul Bracer") && page.contains("Vega"), "{page}");
    assert!(page.contains("class=\"w75\"") && page.contains("38 %"), "3 of 4 is 75 %, the order (0 + 0,75) / 2: {page}");
    let list = text(&browser.get("/admin/orders"));
    assert!(list.contains("class=\"w40\""), "the list shows the bar of the whole order, to five percent: {list}");
    let all = text(&browser.get("/admin/deliveries"));
    assert!(all.contains("3 Kühlmodul Bracer") && all.contains("Vega") && all.contains("2027-01-15 08:01 UTC"), "{all}");

    let removed = browser.post(
        "/admin/orders/edit",
        &[("csrf", csrf.as_str()), ("id", id.as_str()), ("title", "Werft"), ("item", "Laranite"), ("quantity", "2,5"), ("unit", "scu")],
    );
    assert_eq!(removed.status, 422);
    assert!(text(&removed).contains("Kühlmodul Bracer hat schon Lieferungen"));
    let edited = browser.post(
        "/admin/orders/edit",
        &[
            ("csrf", csrf.as_str()),
            ("id", id.as_str()),
            ("title", "Werft"),
            ("item", "Kühlmodul Bracer"),
            ("quantity", "3"),
            ("unit", ""),
            ("item", "Laranite"),
            ("quantity", "1"),
            ("unit", "scu"),
            ("location", ""),
            ("due", ""),
            ("note", ""),
        ],
    );
    assert_eq!(edited.status, 303);
    let order = api_orders(&server, &vega, T0 + 120).into_iter().find(|o| o["id"] == id.as_str()).unwrap();
    assert_eq!(order["title"], "Werft");
    assert_eq!(order["items"][0], json!({"item": "Kühlmodul Bracer", "quantity": 3.0, "delivered": 3.0}));
    assert!(order.get("location").is_none() && order.get("note").is_none(), "emptied fields are gone: {order}");
    assert_eq!(order["status"], "open");

    let closed = browser.post("/admin/orders/close", &[("csrf", csrf.as_str()), ("id", id.as_str())]);
    assert_eq!(closed.status, 303);
    assert!(text(&browser.get(location(&closed))).contains("Geschlossen. Mitglieder können nichts mehr dazu liefern."));
    let order = api_orders(&server, &vega, T0 + 180).into_iter().find(|o| o["id"] == id.as_str()).unwrap();
    assert_eq!(order["status"], "cancelled");
    let late = api(&server, &vega, "POST", &format!("/orders/{id}/deliveries"), r#"{"items":[{"item":"Laranite","quantity":1}]}"#, T0 + 180);
    assert_eq!(late.status, 201, "the first key still answers as the first time");
    let bearer = format!("Bearer {}", vega.access);
    let late = server.handle_at(
        &Request::new(
            "POST",
            &format!("/versekit/v1/orders/{id}/deliveries"),
            &[("Authorization", bearer.as_str()), ("Idempotency-Key", "lieferung-2")],
            br#"{"items":[{"item":"Laranite","quantity":1}]}"#,
        ),
        T0 + 180,
    );
    assert_eq!(late.status, 409, "no delivery to a closed order");
    let edit_closed = browser.post(
        "/admin/orders/edit",
        &[("csrf", csrf.as_str()), ("id", id.as_str()), ("item", "Laranite"), ("quantity", "1"), ("item", "Kühlmodul Bracer"), ("quantity", "3")],
    );
    assert_eq!(edit_closed.status, 409);
    assert_eq!(browser.post("/admin/orders/reopen", &[("csrf", csrf.as_str()), ("id", id.as_str())]).status, 303);
    let order = api_orders(&server, &vega, T0 + 240).into_iter().find(|o| o["id"] == id.as_str()).unwrap();
    assert_eq!(order["status"], "open");
    let list = text(&browser.get("/admin/orders?q=werft"));
    assert!(list.contains(">Werft</a>") && list.contains(">Erz für die Werft</a>") && list.contains("2 von 2"), "{list}");
    let list = text(&browser.get("/admin/orders?q=laranite"));
    assert!(list.contains(">Werft</a>") && !list.contains("Erz für die Werft") && list.contains("1 von 2"), "{list}");
}

#[test]
fn wrong_input_is_refused_and_changes_nothing() {
    let server = server();
    let mut browser = Browser::new(&server);
    browser.log_in(PASSWORD);
    let csrf = browser.csrf();
    let long = "n".repeat(1001);
    let cases: [(&[(&str, &str)], &str); 9] = [
        (&[("item", "Gold"), ("quantity", "0")], "Zeile 1: Die Menge muss eine Zahl über 0 sein."),
        (&[("item", "Gold"), ("quantity", "viel")], "Zeile 1: Die Menge muss eine Zahl über 0 sein."),
        (&[("item", ""), ("quantity", "3")], "Zeile 1: Ware oder Teil fehlt."),
        (&[("item", "Gold"), ("quantity", "1"), ("item", "gold"), ("quantity", "2")], "gold steht zweimal im Auftrag."),
        (&[("item", "Gold"), ("quantity", "1"), ("unit", "kg")], "Zeile 1: unbekannte Einheit."),
        (&[("item", ""), ("quantity", "")], "mindestens eine Ware"),
        (&[("item", "Gold"), ("quantity", "1"), ("due", "2027-02-30")], "Die Frist muss ein Datum sein"),
        (&[("item", "Gold"), ("quantity", "1"), ("title", "a\u{1b}[31m")], "Titel enthält Steuerzeichen."),
        (&[("item", "Gold"), ("quantity", "1"), ("note", long.as_str())], "Notiz ist zu lang (höchstens 1000 Zeichen)."),
    ];
    let before = server_orders(&server);
    for (fields, message) in cases {
        let all = [&[("csrf", csrf.as_str())][..], fields].concat();
        let answer = browser.post("/admin/orders/new", &all);
        assert_eq!(answer.status, 422, "{message}");
        let page = text(&answer);
        assert!(page.contains(message), "{message}: {page}");
        assert!(page.contains("name=\"csrf\""), "the form comes back to correct it");
    }
    assert_eq!(server_orders(&server), before, "no wrong form created an order");

    let status = text(&server.handle_at(&Request::new("GET", "/", &[("Accept-Language", "de")], b""), T0));
    let csrf2 = browser.csrf();
    browser.post(
        "/admin/orders/new",
        &[("csrf", csrf2.as_str()), ("title", "Geheimer Auftrag"), ("item", "Gold"), ("quantity", "1"), ("note", "Nur für die Leitung")],
    );
    let after = text(&server.handle_at(&Request::new("GET", "/", &[("Accept-Language", "de")], b""), T0));
    for secret in ["Geheimer Auftrag", "Nur für die Leitung", "/admin", PASSWORD, browser.session().unwrap()] {
        assert!(!after.contains(secret), "the status page shows {secret:?}");
    }
    assert_eq!(status.replace(char::is_numeric, ""), after.replace(char::is_numeric, ""), "the status page is unchanged");
}

/// The orders as the API gives them to a member who may read them.
fn server_orders(server: &Server) -> Vec<Value> {
    let reader = member(server, "nova", "orders:read", T0);
    api_orders(server, &reader, T0)
}

#[test]
fn members_show_their_rights_and_last_contact_and_can_be_disconnected() {
    let server = server();
    let vega = member(&server, "vega", "stock:read orders:read", T0);
    assert_eq!(api(&server, &vega, "GET", "/me", "", T0 + 120).status, 200);
    let mut browser = Browser::new(&server);
    browser.now = T0 + 300;
    browser.log_in(PASSWORD);
    let page = text(&browser.get("/admin/members"));
    let vega_at = page.find(">Vega<").unwrap();
    let nova_at = page.find(">Nova<").unwrap();
    assert!(vega_at < nova_at, "connected members first");
    let vega_part = &page[vega_at..nova_at];
    assert!(vega_part.contains("Verbunden") && vega_part.contains("Orga-Bestand lesen, Aufträge lesen"), "{vega_part}");
    assert!(vega_part.contains("2027-01-15 08:02 UTC"), "the last contact: {vega_part}");
    assert!(vega_part.contains("aria-label=\"Verbindung von Vega trennen\""));
    let nova_part = &page[nova_at..];
    assert!(nova_part.contains("Nicht verbunden") && nova_part.contains("noch nie") && !nova_part.contains("trennen\""));
    assert!(!page.contains(VEGA_KEY) && !page.contains(&vega.access) && !page.contains(&vega.refresh));

    let found = text(&browser.get("/admin/members?q=NOVA"));
    assert!(found.contains(">Nova<") && !found.contains(">Vega<") && found.contains("1 von 2"), "{found}");

    let csrf = browser.csrf();
    let gone = browser.post("/admin/members/disconnect", &[("csrf", csrf.as_str()), ("member", "vega")]);
    assert_eq!((gone.status, location(&gone)), (303, "/admin/members?getrennt=vega"));
    assert!(text(&browser.get(location(&gone))).contains("Verbindung von Vega getrennt."));
    assert_eq!(api(&server, &vega, "GET", "/me", "", T0 + 360).status, 401, "the access token is void");
    let form = urlencode(&[("grant_type", "refresh_token"), ("refresh_token", vega.refresh.as_str())]);
    let renewed = server.handle_at(&Request::new("POST", "/versekit/v1/token", &[], form.as_bytes()), T0 + 360).json_body();
    assert_eq!(renewed["error"], "invalid_grant", "the refresh token is void");
    let page = text(&browser.get("/admin/members"));
    assert!(page[page.find(">Vega<").unwrap()..].contains("Nicht verbunden"));
    assert_eq!(browser.post("/admin/members/disconnect", &[("csrf", csrf.as_str()), ("member", "niemand")]).status, 404);
}

#[test]
fn stock_blueprints_and_deliveries_are_listed_and_searched() {
    let server = server();
    let vega = member(&server, "vega", "stock:write blueprints:write", T0);
    let nova = member(&server, "nova", "stock:write blueprints:write", T0);
    let put = |who: &Member, path: &str, body: &str| {
        let bearer = format!("Bearer {}", who.access);
        let answer = server.handle_at(&Request::new("PUT", &format!("/versekit/v1{path}"), &[("Authorization", bearer.as_str())], body.as_bytes()), T0);
        assert_eq!(answer.status, 200, "{}", String::from_utf8_lossy(&answer.body));
    };
    put(&vega, "/stock/mine", r#"{"items":[{"item":"Quantanium","quantity":12.5,"unit":"scu","quality":870,"location":"Area18"}]}"#);
    put(&nova, "/stock/mine", r#"{"items":[{"item":"Laranite","quantity":3,"unit":"scu","location":"Lorville"}]}"#);
    put(&vega, "/blueprints/mine", r#"{"blueprints":["Arrowhead Sniper Rifle","Karna Rifle"]}"#);
    put(&nova, "/blueprints/mine", r#"{"blueprints":["Karna Rifle"]}"#);

    let mut browser = Browser::new(&server);
    browser.log_in(PASSWORD);
    let stock = text(&browser.get("/admin/stock"));
    assert!(stock.contains("Quantanium") && stock.contains("12,5 SCU") && stock.contains("Qualität 870") && stock.contains("Vega"));
    assert!(stock.contains("Laranite") && stock.contains("Nova") && stock.contains("2 von 2"));
    let found = text(&browser.get("/admin/stock?q=area18"));
    assert!(found.contains("Quantanium") && !found.contains("Laranite") && found.contains("1 von 2"), "{found}");
    assert!(found.contains("Filter löschen"));
    browser.language = "en";
    assert!(text(&browser.get("/admin/stock")).contains("12.5 SCU"), "English writes a point");
    browser.language = "de";

    let blueprints = text(&browser.get("/admin/blueprints"));
    assert!(blueprints.contains("Karna Rifle") && blueprints.contains("Vega, Nova") || blueprints.contains("Nova, Vega"), "{blueprints}");
    let found = text(&browser.get("/admin/blueprints?q=nova"));
    assert!(found.contains("Karna Rifle") && !found.contains("Arrowhead"), "{found}");

    let overview = text(&browser.get("/admin"));
    assert!(overview.contains(">2 / 2<") && overview.contains(">2<"), "{overview}");
    let none = text(&browser.get("/admin/deliveries?q=nichts"));
    assert!(none.contains("Nichts gefunden.") && none.contains("0 von 0"));
}
