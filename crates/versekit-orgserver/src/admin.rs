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

//! The pages of the org behind two doors, in the look of the other pages: `/admin` with the admin password of the
//! environment (the way in for the setup and when Discord fails) and `/portal` with the Discord login of a member
//! (`portal.rs`). Both doors show the same pages; what a visitor sees and may do follows the role.
//!
//! | Part | Does |
//! |---|---|
//! | access | `/admin`: one admin password from the environment, kept as a PBKDF2 hash; a session in memory behind a cookie (HttpOnly, SameSite=Strict, `Path=/admin`, Secure under https); log out; after five wrong passwords the login waits, longer each time. The password counts as the role lead |
//! | roles | member: overview, stock, blueprints, orders, deliveries; logistics: also create, edit, close orders and confirm deliveries; lead: also the connected members, the access to the portal and its settings |
//! | forms | every form carries the session's CSRF token; a POST from another origin or another site is refused |
//! | overviews | connected members with their rights and last contact (disconnect by button), the org's stock, blueprints and deliveries, each with a search |
//! | orders | create, edit, close and reopen; the deliveries booked show at the order with the progress and who confirmed them |
//!
//! Without a password `/admin` is off and says how to turn it on. Nothing of it shows on the status page.

use std::collections::HashMap;
use std::num::NonZeroU32;

use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Value, json};

use crate::http::{Request, Response, escape_html, value};
use crate::look::{german, icon, page as frame, pick};
use crate::panel::{Tone, note_box, scope_label};
use crate::portal::{self, Role};
use crate::status::moment;
use crate::{MAX_TEXT, SCOPES, Server, State, hash};

/// The root of the administration.
pub const ROOT: &str = "/admin";
/// The cookie of a session.
pub const COOKIE: &str = "vk_admin";
/// The cookie that ties the login form to the browser that loaded it.
pub const LOGIN_COOKIE: &str = "vk_admin_login";
/// How long a session lasts.
pub const SESSION_SECONDS: u64 = 8 * 3600;
/// Wrong passwords before the login waits.
pub const FREE_TRIES: u32 = 5;
/// The shortest admin password.
pub const MIN_PASSWORD: usize = 12;
/// The longest wait after wrong passwords.
pub const LOCK_MAX: u64 = 900;
/// The most goods of one order.
pub const MAX_LINES: usize = 50;
/// The longest note of an order.
pub const MAX_NOTE: usize = 1000;
const LOGIN_FORM_SECONDS: u64 = 600;
const ITERATIONS: u32 = 600_000;
const MAX_ROWS: usize = 200;
const EMPTY_ROWS: usize = 3;
const MIN_ROWS: usize = 5;

/// The admin password as a salted PBKDF2-SHA256 hash; `Debug` shows nothing of it.
#[derive(Clone)]
pub(crate) struct AdminKey {
    salt: [u8; 16],
    derived: [u8; 32],
}

impl std::fmt::Debug for AdminKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdminKey(hidden)")
    }
}

fn iterations() -> NonZeroU32 {
    NonZeroU32::new(ITERATIONS).unwrap_or(NonZeroU32::MIN)
}

impl AdminKey {
    /// The hash of `password` with a fresh salt; the error says what is wrong without the password.
    pub(crate) fn new(password: &str) -> Result<AdminKey, String> {
        if password.chars().count() < MIN_PASSWORD {
            return Err(format!("the admin password is shorter than {MIN_PASSWORD} characters"));
        }
        if password.len() > 1024 {
            return Err("the admin password is longer than 1024 bytes".into());
        }
        let mut salt = [0u8; 16];
        SystemRandom::new().fill(&mut salt).map_err(|_| "the system gives no random numbers".to_string())?;
        let mut derived = [0u8; 32];
        pbkdf2::derive(pbkdf2::PBKDF2_HMAC_SHA256, iterations(), &salt, password.as_bytes(), &mut derived);
        Ok(AdminKey { salt, derived })
    }

    /// Whether `typed` is the password, compared in constant time.
    fn verify(&self, typed: &str) -> bool {
        typed.len() <= 1024
            && pbkdf2::verify(pbkdf2::PBKDF2_HMAC_SHA256, iterations(), &self.salt, typed.as_bytes(), &self.derived).is_ok()
    }
}

/// One session of the administration.
#[derive(Clone, Debug)]
pub(crate) struct Session {
    expires: u64,
    csrf: String,
}

/// The sessions and the brake of the login; kept in memory only.
#[derive(Default)]
pub(crate) struct AdminState {
    sessions: HashMap<String, Session>,
    fails: u32,
    locked_until: u64,
    /// The login forms handed out: hash of the token and when it runs out; each counts once.
    forms: Vec<(String, u64)>,
}

/// Login forms kept at the same time; the oldest goes first.
const FORMS_MAX: usize = 500;

/// Who acts on a page.
#[derive(Clone, Debug)]
pub(crate) enum Actor {
    /// Logged in with the admin password.
    Admin,
    /// Logged in through Discord: the Discord id and the name shown.
    Member { id: String, name: String },
}

/// What every page of a session needs.
pub(crate) struct Ctx<'a> {
    pub(crate) server: &'a Server,
    pub(crate) request: &'a Request,
    pub(crate) de: bool,
    pub(crate) csrf: String,
    pub(crate) now: u64,
    /// `/admin` or `/portal`: where the pages of this session live.
    pub(crate) base: &'static str,
    pub(crate) role: Role,
    pub(crate) actor: Actor,
}

impl Ctx<'_> {
    pub(crate) fn say<'b>(&self, de: &'b str, en: &'b str) -> &'b str {
        pick(self.de, de, en)
    }

    pub(crate) fn query(&self, name: &str) -> String {
        value(&self.request.query_pairs(), name).unwrap_or_default().trim().to_string()
    }

    pub(crate) fn hidden(&self) -> String {
        format!("<input type=\"hidden\" name=\"csrf\" value=\"{}\">", escape_html(&self.csrf))
    }

    /// The address of the page `sub` behind this session's door.
    pub(crate) fn at(&self, sub: &str) -> String {
        format!("{}{sub}", self.base)
    }

    /// The name that stands for the actor, as kept with a confirmation.
    pub(crate) fn actor_name(&self) -> String {
        match &self.actor {
            Actor::Admin => self.say("Verwaltung (Admin-Passwort)", "Administration (admin password)").to_string(),
            Actor::Member { name, .. } => name.clone(),
        }
    }
}

/// The parts of the pages: address behind the door, icon, German and English name, the role that sees the part.
const SECTIONS: [(&str, &str, &str, &str, Role); 8] = [
    ("", "layout-dashboard", "Übersicht", "Overview", Role::Member),
    ("/stock", "warehouse", "Bestand", "Stock", Role::Member),
    ("/blueprints", "hammer", "Baupläne", "Blueprints", Role::Member),
    ("/orders", "clipboard-list", "Aufträge", "Orders", Role::Member),
    ("/deliveries", "package-plus", "Lieferungen", "Deliveries", Role::Member),
    ("/members", "users", "Mitglieder", "Members", Role::Lead),
    ("/access", "user-check", "Zugang", "Access", Role::Lead),
    ("/settings", "settings", "Einstellungen", "Settings", Role::Lead),
];

/// The role a page behind a door asks for; `None` for an address that is no page.
pub(crate) fn needed(sub: &str) -> Option<Role> {
    Some(match sub {
        "" | "/stock" | "/blueprints" | "/orders" | "/orders/view" | "/deliveries" | "/logout" => Role::Member,
        "/orders/new" | "/orders/edit" | "/orders/close" | "/orders/reopen" | "/deliveries/confirm" => Role::Logistics,
        "/members" | "/members/disconnect" | "/access" | "/access/role" | "/access/forget" | "/settings" => Role::Lead,
        _ => return None,
    })
}

/// Whether `path` belongs to the administration.
pub(crate) fn owns(path: &str) -> bool {
    path == ROOT || path.starts_with("/admin/")
}

pub(crate) fn redirect(location: &str) -> Response {
    Response {
        status: 303,
        headers: vec![("Location".into(), location.into()), ("Cache-Control".into(), "no-store".into())],
        body: Vec::new(),
    }
}

pub(crate) fn language(response: Response, de: bool) -> Response {
    response.with_header("Content-Language", pick(de, "de", "en")).with_header("Vary", "Accept-Language")
}

/// A page outside a session: the login, the note that the administration is off, a refusal.
pub(crate) fn plain(server: &Server, de: bool, status: u16, title: &str, body: &str) -> Response {
    let config = server.config();
    let page = frame(de, &config.name, &config.tag, title, &format!("<h1>{}</h1>{body}", escape_html(title)));
    language(Response::html(status, page), de)
}

/// A refusal outside the pages, with the way back to the door `base`.
pub(crate) fn refused_at(server: &Server, de: bool, status: u16, text: &str, base: &str) -> Response {
    let (title, back) = if base == ROOT {
        (pick(de, "Verwaltung", "Administration"), pick(de, "Zur Verwaltung", "To the administration"))
    } else {
        (pick(de, "Portal", "Portal"), pick(de, "Zum Portal", "To the portal"))
    };
    let body = format!(
        "{}<div class=\"knopfreihe\"><a class=\"knopf\" href=\"{base}\">{}{back}</a></div>",
        note_box(Tone::Warn, text),
        icon("arrow-left"),
    );
    plain(server, de, status, title, &body)
}

fn refused(server: &Server, de: bool, status: u16, text: &str) -> Response {
    refused_at(server, de, status, text, ROOT)
}

fn off(server: &Server, de: bool) -> Response {
    let body = format!(
        "{note}<section class=\"karte\"><h2>{info}{head}</h2><p>{how}</p></section>\
         <div class=\"knopfreihe\"><a class=\"knopf\" href=\"/\">{back}{overview}</a></div>",
        note = note_box(Tone::Warn, pick(de, "Die Verwaltung ist aus.", "The administration is off.")),
        info = icon("info"),
        head = pick(de, "So schaltest du sie ein", "How to turn it on"),
        how = pick(
            de,
            "Sie wurde mit <code>VERSEKIT_ORG_ADMIN=off</code> ausgeschaltet. Nimm die Zeile heraus und starte den Server neu: Er erzeugt dann selbst ein Passwort im Datenordner. Ein eigenes setzt <code>VERSEKIT_ORG_ADMIN_PASSWORD</code> (mindestens 12 Zeichen, am besten in der Datei <code>.env</code> neben der Compose-Datei).",
            "It was turned off with <code>VERSEKIT_ORG_ADMIN=off</code>. Take the line out and restart the server: it then makes a password in the data folder itself. Your own one is set with <code>VERSEKIT_ORG_ADMIN_PASSWORD</code> (at least 12 characters, best in the file <code>.env</code> next to the compose file).",
        ),
        back = icon("arrow-left"),
        overview = pick(de, "Zur Übersicht", "To the overview"),
    );
    plain(server, de, 404, pick(de, "Verwaltung", "Administration"), &body)
}

/// The value of the cookie `name`, if it is set and not empty.
pub(crate) fn cookie<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case("Cookie"))
        .flat_map(|(_, v)| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(n, _)| *n == name)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

fn cookie_attributes(server: &Server) -> String {
    let secure = if server.config().public_url.starts_with("https://") { "; Secure" } else { "" };
    format!("; Path={ROOT}; HttpOnly; SameSite=Strict{secure}")
}

/// Whether two secrets are equal; the hashes are compared, so the time says nothing about the secret.
pub(crate) fn same_secret(a: &str, b: &str) -> bool {
    hash(a) == hash(b)
}

/// Whether a form comes from a page of this server: neither another site nor a neighbour of the same site, and an
/// `Origin` of the same host when the browser names one.
pub(crate) fn same_origin(request: &Request) -> bool {
    if request
        .header("Sec-Fetch-Site")
        .is_some_and(|site| site.eq_ignore_ascii_case("cross-site") || site.eq_ignore_ascii_case("same-site"))
    {
        return false;
    }
    match request.header("Origin") {
        None => true,
        Some(origin) => {
            let host = request.header("Host").unwrap_or_default();
            origin.split_once("://").is_some_and(|(_, rest)| !host.is_empty() && rest.eq_ignore_ascii_case(host))
        }
    }
}

/// Whether the browser sends `Origin: null` and does not say the form comes from another site. Browsers do so for
/// the forms of a page with `Referrer-Policy: no-referrer`; the form's own token is the proof of where it comes from,
/// since another site cannot read the token.
pub(crate) fn null_origin(request: &Request) -> bool {
    request.header("Origin").is_some_and(|origin| origin == "null")
        && !request
            .header("Sec-Fetch-Site")
            .is_some_and(|site| site.eq_ignore_ascii_case("cross-site") || site.eq_ignore_ascii_case("same-site"))
}

/// Whether a POST carries the session's CSRF token and comes from a page of this server.
pub(crate) fn form_ok(request: &Request, csrf: &str) -> bool {
    let form = request.form();
    let given = value(&form, "csrf").unwrap_or_default();
    (same_origin(request) || null_origin(request)) && !given.is_empty() && !csrf.is_empty() && same_secret(given, csrf)
}

/// The text of a refused form.
pub(crate) fn stale(de: bool) -> &'static str {
    pick(
        de,
        "Das Formular ist abgelaufen oder kam nicht von dieser Seite. Lade die Seite neu und versuch es noch einmal.",
        "The form has expired or did not come from this page. Reload the page and try again.",
    )
}

/// Answers every request under `/admin`.
pub(crate) fn handle(server: &Server, request: &Request, now: u64) -> Response {
    let de = german(request);
    let Some(key) = server.config().admin.as_ref() else { return off(server, de) };
    let path = request.path.as_str();
    let method = request.method.as_str();
    if path == "/admin/login" {
        return match method {
            "GET" => {
                let out = value(&request.query_pairs(), "out").is_some();
                let note = out.then(|| (Tone::Good, pick(de, "Abgemeldet.", "Logged out.")));
                login_form(server, de, note, 200, now)
            }
            "POST" => login(server, key, request, de, now),
            _ => crate::not_allowed(),
        };
    }
    let csrf = {
        let mut state = server.lock();
        state.admin.sessions.retain(|_, s| s.expires > now);
        cookie(request, COOKIE).and_then(|token| state.admin.sessions.get(&hash(token)).map(|s| s.csrf.clone()))
    };
    let Some(csrf) = csrf else {
        return if method == "GET" {
            redirect("/admin/login")
        } else {
            refused(server, de, 403, pick(de, "Nicht angemeldet.", "Not logged in."))
        };
    };
    if method == "POST" && !form_ok(request, &csrf) {
        return refused(server, de, 403, stale(de));
    }
    let ctx = Ctx { server, request, de, csrf, now, base: ROOT, role: Role::Lead, actor: Actor::Admin };
    let sub = &path[ROOT.len()..];
    if sub == "/logout" {
        return if method == "POST" { logout(&ctx) } else { crate::not_allowed() };
    }
    pages(&ctx, sub)
}

/// Answers a page behind a door after the session and, for a POST, its CSRF token were checked: first the role the
/// page asks for, then the method.
pub(crate) fn pages(ctx: &Ctx, sub: &str) -> Response {
    let Some(need) = needed(sub) else {
        return shell(ctx, "", ctx.say("Nicht gefunden", "Not found"), &missing(ctx), 404);
    };
    if ctx.role < need {
        let text = ctx
            .say("Diese Seite ist für die Rolle {r} und höher.", "This page is for the role {r} and above.")
            .replace("{r}", portal::role_label(need, ctx.de));
        return shell(ctx, "", ctx.say("Kein Zugriff", "No access"), &note_box(Tone::Warn, &text), 403);
    }
    match (ctx.request.method.as_str(), sub) {
        ("GET", "") => overview(ctx),
        ("GET", "/members") => members(ctx),
        ("POST", "/members/disconnect") => disconnect(ctx),
        ("GET", "/stock") => stock(ctx),
        ("GET", "/blueprints") => blueprints(ctx),
        ("GET", "/orders") => orders(ctx),
        ("GET", "/orders/new") => order_form(ctx, None, &Draft::default(), None, 200),
        ("POST", "/orders/new") => save_order(ctx, false),
        ("GET", "/orders/view") => order_view(ctx),
        ("GET", "/orders/edit") => edit_form(ctx),
        ("POST", "/orders/edit") => save_order(ctx, true),
        ("POST", "/orders/close") => set_status(ctx, false),
        ("POST", "/orders/reopen") => set_status(ctx, true),
        ("GET", "/deliveries") => deliveries(ctx),
        ("POST", "/deliveries/confirm") => confirm_delivery(ctx),
        ("GET", "/access") => portal::access_page(ctx),
        ("POST", "/access/role") => portal::set_role(ctx),
        ("POST", "/access/forget") => portal::forget(ctx),
        ("GET", "/settings") => portal::settings_page(ctx, None, 200),
        ("POST", "/settings") => portal::save_settings(ctx),
        _ => crate::not_allowed(),
    }
}

/// The login page with a fresh token, kept on the server and tied to the browser by a cookie.
fn login_form(server: &Server, de: bool, note: Option<(Tone, &str)>, status: u16, now: u64) -> Response {
    let token = server.random_hex(16);
    {
        let mut state = server.lock();
        let forms = &mut state.admin.forms;
        forms.retain(|(_, expires)| *expires > now);
        if forms.len() >= FORMS_MAX {
            forms.remove(0);
        }
        forms.push((hash(&token), now + LOGIN_FORM_SECONDS));
    }
    let portal = if server.config().discord.is_some() {
        format!(
            "<p class=\"hinweis\">{} <a href=\"{}\">{}</a></p>",
            pick(de, "Mitglieder und Leitung melden sich im Alltag mit Discord an:", "Members and leadership log in with Discord day to day:"),
            portal::ROOT,
            pick(de, "zum Portal", "to the portal"),
        )
    } else {
        String::new()
    };
    let body = format!(
        "<p class=\"einleitung\">{intro}</p>{note}\
         <section class=\"karte\"><h2>{lock}{head}</h2>\
         <form method=\"post\" action=\"/admin/login\"><input type=\"hidden\" name=\"csrf\" value=\"{token}\">\
         <input type=\"text\" name=\"username\" value=\"admin\" autocomplete=\"username\" hidden>\
         <label class=\"feld\" for=\"p\">{label}</label>\
         <input id=\"p\" type=\"password\" name=\"password\" autocomplete=\"current-password\" required>\
         <div class=\"knopfreihe\"><button class=\"knopf voll\" type=\"submit\">{open}{go}</button></div>\
         </form><p class=\"hinweis\">{expiry} {brake}</p>{portal}</section>",
        expiry = pick(
            de,
            "Das Formular gilt 10 Minuten; danach zeigt die Seite ein frisches.",
            "The form is valid for 10 minutes; after that the page shows a fresh one.",
        ),
        intro = pick(
            de,
            "Notzugang der Orga-Leitung mit dem Admin-Passwort: für die Einrichtung und wenn Discord klemmt.",
            "Emergency access of the org's leadership with the admin password: for the setup and when Discord fails.",
        ),
        note = note.map(|(tone, text)| note_box(tone, text)).unwrap_or_default(),
        lock = icon("lock"),
        head = pick(de, "Anmelden", "Log in"),
        label = pick(de, "Admin-Passwort", "Admin password"),
        open = icon("lock-open"),
        go = pick(de, "Anmelden", "Log in"),
        brake = pick(
            de,
            "Nach fünf falschen Passwörtern wartet die Anmeldung eine Minute, danach jedes Mal länger.",
            "After five wrong passwords the login waits a minute, longer each time after that.",
        ),
    );
    plain(server, de, status, pick(de, "Verwaltung", "Administration"), &body).with_header(
        "Set-Cookie",
        &format!("{LOGIN_COOKIE}={token}; Max-Age={LOGIN_FORM_SECONDS}{}", cookie_attributes(server)),
    )
}

fn locked(server: &Server, de: bool, wait: u64, now: u64) -> Response {
    let minutes = wait.div_ceil(60).max(1);
    let text = pick(
        de,
        "Zu viele falsche Passwörter. Versuch es in {n} Min. wieder.",
        "Too many wrong passwords. Try again in {n} min.",
    )
    .replace("{n}", &minutes.to_string());
    login_form(server, de, Some((Tone::Warn, &text)), 429, now).with_header("Retry-After", &wait.to_string())
}

/// Whether the browser itself says the form comes from this origin: `Sec-Fetch-Site: same-origin`, or without that
/// header an `Origin` (checked against the host by [`same_origin`]).
fn vouched(request: &Request) -> bool {
    match request.header("Sec-Fetch-Site") {
        Some(site) => site.eq_ignore_ascii_case("same-origin"),
        None => request.header("Origin").is_some(),
    }
}

fn login(server: &Server, key: &crate::admin::AdminKey, request: &Request, de: bool, now: u64) -> Response {
    let form = request.form();
    let field = value(&form, "csrf").unwrap_or_default();
    let tied = cookie(request, LOGIN_COOKIE).is_some_and(|c| !field.is_empty() && same_secret(c, field));
    // The token counts once: taken from the list of forms handed out whichever way the form is tied.
    let listed = {
        let mut state = server.lock();
        let forms = &mut state.admin.forms;
        forms.retain(|(_, expires)| *expires > now);
        let wanted = hash(field);
        match forms.iter().position(|(h, _)| !field.is_empty() && *h == wanted) {
            Some(index) => {
                forms.remove(index);
                true
            }
            None => false,
        }
    };
    // `Origin: null` comes with a token this server handed out or the cookie tied to it: another site knows neither.
    let null = null_origin(request) && (listed || tied);
    if !same_origin(request) && !null {
        let origin = request.header("Origin").unwrap_or("–");
        let host = request.header("Host").unwrap_or("–");
        eprintln!("admin login refused: the form came from {origin}, the address is {host}");
        let foreign = pick(
            de,
            "Das Formular kam nicht von dieser Seite (Herkunft {o}, Adresse {h}). Öffne die Verwaltung über genau eine Adresse und gib das Passwort hier noch einmal ein.",
            "The form did not come from this page (origin {o}, address {h}). Open the administration through one address only and type the password here again.",
        )
        .replace("{o}", origin)
        .replace("{h}", host);
        return login_form(server, de, Some((Tone::Warn, &foreign)), 403, now);
    }
    // Tied to this browser by the cookie, or, where a browser holds the cookie back, a fresh token of this server
    // with the browser's own word that the form comes from this origin. Else a fresh form; no password is checked.
    if !tied && !(listed && (vouched(request) || null)) {
        eprintln!(
            "admin login: fresh form ({}, {})",
            if cookie(request, LOGIN_COOKIE).is_some() { "cookie of another form" } else { "no form cookie" },
            if listed { "token known, but no Sec-Fetch-Site or Origin" } else { "token unknown or older than 10 minutes" },
        );
        let fresh = pick(
            de,
            "Die Anmeldeseite war länger als 10 Minuten offen. Hier ist eine frische: Gib das Passwort noch einmal ein.",
            "The login page was open for more than 10 minutes. Here is a fresh one: type the password again.",
        );
        return login_form(server, de, Some((Tone::Info, fresh)), 200, now);
    }
    // One check at a time: the brake is read after waiting for the turn, so parallel tries cannot pass it.
    let _turn = server.admin_gate.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let until = server.lock().admin.locked_until;
    if until > now {
        return locked(server, de, until - now, now);
    }
    let good = key.verify(value(&form, "password").unwrap_or_default());
    let mut state = server.lock();
    if !good {
        state.admin.fails += 1;
        if state.admin.fails >= FREE_TRIES {
            let wait = (60u64 << (state.admin.fails - FREE_TRIES).min(4)).min(LOCK_MAX);
            state.admin.locked_until = now + wait;
            drop(state);
            return locked(server, de, wait, now);
        }
        drop(state);
        return login_form(server, de, Some((Tone::Warn, pick(de, "Das Passwort stimmt nicht.", "The password is wrong."))), 401, now);
    }
    state.admin.fails = 0;
    state.admin.locked_until = 0;
    let token = server.random_hex(32);
    let csrf = server.random_hex(16);
    state.admin.sessions.retain(|_, s| s.expires > now);
    state.admin.sessions.insert(hash(&token), Session { expires: now + SESSION_SECONDS, csrf });
    drop(state);
    let attributes = cookie_attributes(server);
    redirect(ROOT)
        .with_header("Set-Cookie", &format!("{COOKIE}={token}; Max-Age={SESSION_SECONDS}{attributes}"))
        .with_header("Set-Cookie", &format!("{LOGIN_COOKIE}=; Max-Age=0{attributes}"))
}

fn logout(ctx: &Ctx) -> Response {
    if let Some(token) = cookie(ctx.request, COOKIE) {
        ctx.server.lock().admin.sessions.remove(&hash(token));
    }
    redirect("/admin/login?out=1").with_header("Set-Cookie", &format!("{COOKIE}=; Max-Age=0{}", cookie_attributes(ctx.server)))
}

/// A page of a session: the tabs the role may see, who is logged in with the log-out button, the title and `body`.
/// `active` is the part behind the door whose tab is marked.
pub(crate) fn shell(ctx: &Ctx, active: &str, title: &str, body: &str, status: u16) -> Response {
    let tabs: String = SECTIONS
        .iter()
        .filter(|(.., role)| ctx.role >= *role)
        .map(|(path, symbol, de, en, _)| {
            format!(
                "<a href=\"{href}\"{current}>{icon}<span>{label}</span></a>",
                href = ctx.at(path),
                current = if *path == active { " aria-current=\"page\"" } else { "" },
                icon = icon(symbol),
                label = pick(ctx.de, de, en),
            )
        })
        .collect();
    let who = match &ctx.actor {
        Actor::Admin => ctx.say("Notzugang mit dem Admin-Passwort", "Emergency access with the admin password").to_string(),
        Actor::Member { name, .. } => format!("{} · {}", escape_html(name), portal::role_label(ctx.role, ctx.de)),
    };
    let full = format!(
        "<nav class=\"reiter\" aria-label=\"{aria}\">{tabs}\
         <form method=\"post\" action=\"{logout}\">{hidden}<button type=\"submit\">{door}<span>{out}</span></button></form></nav>\
         <p class=\"wer\">{user}<span>{who}</span></p><h1>{title}</h1>{body}",
        aria = ctx.say("Bereiche", "Sections"),
        logout = ctx.at("/logout"),
        hidden = ctx.hidden(),
        door = icon("door-open"),
        out = ctx.say("Abmelden", "Log out"),
        user = icon("user"),
        title = escape_html(title),
    );
    let config = ctx.server.config();
    let door = if ctx.base == ROOT { ctx.say("Verwaltung", "Administration") } else { "Portal" };
    let head = format!("{title} · {door}");
    language(Response::html(status, frame(ctx.de, &config.name, &config.tag, &head, &full)), ctx.de)
}

pub(crate) fn missing(ctx: &Ctx) -> String {
    note_box(Tone::Warn, ctx.say("Diese Seite oder dieser Auftrag existiert nicht.", "This page or order does not exist."))
}

/// A number as people write it: no decimals when whole, at most three, a comma in German.
fn number(quantity: f64, de: bool) -> String {
    let text = if quantity.fract() == 0.0 && quantity.abs() < 1e15 {
        format!("{}", quantity as i64)
    } else {
        format!("{quantity:.3}").trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if de { text.replace('.', ",") } else { text }
}

fn unit_label(unit: &str, de: bool) -> String {
    match unit {
        "" | "piece" => pick(de, "Stk.", "pcs").to_string(),
        "scu" => "SCU".into(),
        "cscu" => "cSCU".into(),
        other => other.to_string(),
    }
}

fn text<'a>(object: &'a Value, key: &str) -> &'a str {
    object.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn amount(object: &Value, key: &str) -> f64 {
    object.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn lines_of(order: &Value) -> &[Value] {
    order.get("items").and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// The share delivered of an order, each good counted alike and at most in full.
fn progress(order: &Value) -> f64 {
    let lines = lines_of(order);
    if lines.is_empty() {
        return 0.0;
    }
    let sum: f64 = lines
        .iter()
        .map(|l| {
            let wanted = amount(l, "quantity");
            if wanted > 0.0 { (amount(l, "delivered") / wanted).min(1.0) } else { 1.0 }
        })
        .sum();
    sum / lines.len() as f64
}

fn bar(share: f64) -> String {
    format!("<div class=\"balken\" aria-hidden=\"true\"><span class=\"{}\"></span></div>", crate::look::bar_class(share))
}

fn status_label(status: &str, de: bool) -> (&'static str, &'static str) {
    match status {
        "open" => (pick(de, "Offen", "Open"), "an"),
        "done" => (pick(de, "Erledigt", "Done"), "fertig"),
        _ => (pick(de, "Geschlossen", "Closed"), "aus"),
    }
}

pub(crate) fn badge(label: &str, class: &str) -> String {
    format!("<span class=\"marke {class}\">{}</span>", escape_html(label))
}

/// Whether `needle` (lower case) is empty or found in one of `fields`.
pub(crate) fn found(needle: &str, fields: &[&str]) -> bool {
    needle.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(needle))
}

/// The search above a list; `extra` holds more fields, `filtered` shows the way back to the whole list.
pub(crate) fn search_form(ctx: &Ctx, action: &str, q: &str, extra: &str, filtered: bool, placeholder: &str) -> String {
    let reset = if filtered {
        format!("<a class=\"knopf klein\" href=\"{action}\">{}{}</a>", icon("funnel-x"), ctx.say("Filter löschen", "Clear filter"))
    } else {
        String::new()
    };
    format!(
        "<form class=\"suche\" method=\"get\" action=\"{action}\" role=\"search\">\
         <label class=\"unsichtbar\" for=\"q\">{label}</label>\
         <input id=\"q\" type=\"search\" name=\"q\" value=\"{q}\" placeholder=\"{placeholder}\">{extra}\
         <button class=\"knopf klein\" type=\"submit\">{search}{go}</button>{reset}</form>",
        label = ctx.say("Suche", "Search"),
        q = escape_html(q),
        placeholder = escape_html(placeholder),
        search = icon("search"),
        go = ctx.say("Suchen", "Search"),
    )
}

pub(crate) fn count_line(ctx: &Ctx, shown: usize, total: usize) -> String {
    let text = ctx.say("{a} von {b}", "{a} of {b}").replace("{a}", &shown.to_string()).replace("{b}", &total.to_string());
    format!("<p class=\"zahl\">{text}</p>")
}

pub(crate) fn list_card(symbol: &str, head: &str, rows: &[String], empty: &str) -> String {
    let inner = if rows.is_empty() {
        format!("<p class=\"leer\">{}</p>", escape_html(empty))
    } else {
        format!("<ul class=\"zeilen\">{}</ul>", rows.concat())
    };
    format!("<section class=\"karte\"><h2>{}{}</h2>{inner}</section>", icon(symbol), escape_html(head))
}

/// The ids of the members holding a login, with the rights of their logins in the order of the standard.
fn logins(state: &State) -> HashMap<&str, Vec<&'static str>> {
    let mut held: HashMap<&str, Vec<&'static str>> = HashMap::new();
    for grant in state.refresh.values() {
        let rights = held.entry(grant.member.as_str()).or_default();
        for scope in SCOPES {
            if grant.scopes.iter().any(|s| s == scope) && !rights.contains(&scope) {
                rights.push(scope);
            }
        }
    }
    for rights in held.values_mut() {
        rights.sort_by_key(|s| SCOPES.iter().position(|x| x == s));
    }
    held
}

fn overview(ctx: &Ctx) -> Response {
    let config = ctx.server.config();
    let state = ctx.server.lock();
    let held = logins(&state);
    let connected = config.members.iter().filter(|m| held.contains_key(m.id.as_str())).count();
    let stock: usize = state.stock.values().map(Vec::len).sum();
    let mut names: Vec<&str> = state.blueprints.values().flatten().map(String::as_str).collect();
    names.sort_unstable();
    names.dedup();
    let open = state.orders.iter().filter(|o| text(o, "status") == "open").count();
    let unconfirmed = state.delivery_log.iter().filter(|d| d.get("confirmed").is_none()).count();
    let waiting = portal::waiting(&state);
    let lead = ctx.role >= Role::Lead;
    let mut tiles = Vec::new();
    if lead {
        tiles.push(("/members", "users", ctx.say("Mitglieder verbunden", "Members connected"), format!("{connected} / {}", config.members.len())));
    }
    tiles.push(("/stock", "warehouse", ctx.say("Einträge im Bestand", "Entries in stock"), stock.to_string()));
    tiles.push(("/blueprints", "hammer", ctx.say("Baupläne", "Blueprints"), names.len().to_string()));
    tiles.push(("/orders", "clipboard-list", ctx.say("Offene Aufträge", "Open orders"), open.to_string()));
    tiles.push(("/deliveries", "package-plus", ctx.say("Lieferungen unbestätigt", "Deliveries unconfirmed"), unconfirmed.to_string()));
    if lead && (waiting > 0 || config.discord.is_some()) {
        tiles.push(("/access", "user-check", ctx.say("Warten auf Freischaltung", "Waiting for access"), waiting.to_string()));
    }
    drop(state);
    let tiles: String = tiles
        .iter()
        .map(|(path, symbol, label, count)| {
            format!(
                "<a class=\"karte kachel\" href=\"{href}\"><span class=\"kachel-kopf\">{icon}{label}</span><span class=\"kachel-zahl\">{count}</span></a>",
                href = ctx.at(path),
                icon = icon(symbol),
                label = escape_html(label),
            )
        })
        .collect();
    if !lead {
        let body = format!(
            "<p class=\"einleitung\">{intro}</p><div class=\"kacheln\">{tiles}</div>{new}",
            intro = ctx.say(
                "Hier siehst du, was die Orga hat und braucht. Bestand, Baupläne und Lieferungen teilen die Mitglieder aus VerseKit.",
                "Here you see what the org has and needs. Members share stock, blueprints and deliveries from VerseKit.",
            ),
            new = if ctx.role >= Role::Logistics {
                format!(
                    "<div class=\"knopfreihe\"><a class=\"knopf voll\" href=\"{}\">{}{}</a></div>",
                    ctx.at("/orders/new"),
                    icon("plus"),
                    ctx.say("Neuer Auftrag", "New order")
                )
            } else {
                String::new()
            },
        );
        return shell(ctx, "", ctx.say("Übersicht", "Overview"), &body, 200);
    }
    let update = ctx.server.update_state();
    let offer = update
        .offer
        .map(|offer| {
            format!(
                "<div class=\"meldung warn\" role=\"status\">{info}<span>{text} <a href=\"{url}\">{changes}</a></span></div>",
                info = icon("info"),
                text = escape_html(&ctx.say("Update verfügbar: {v}.", "Update available: {v}.").replace("{v}", &offer.version)),
                url = escape_html(&offer.url),
                changes = ctx.say("Was ist neu?", "What is new?"),
            )
        })
        .unwrap_or_default();
    let checks: String = crate::setup::findings(ctx.server, ctx.now)
        .iter()
        .map(|f| {
            format!(
                "<li{class}>{icon}<span>{text}</span></li>",
                class = if f.ok { "" } else { " class=\"rot\"" },
                icon = icon(if f.ok { "check" } else { "triangle-alert" }),
                text = escape_html(if ctx.de { &f.de } else { &f.en }),
            )
        })
        .collect();
    let body = format!(
        "{offer}<p class=\"einleitung\">{intro}</p>{emergency}<div class=\"kacheln\">{tiles}</div>\
         <div class=\"knopfreihe\"><a class=\"knopf voll\" href=\"{new_href}\">{plus}{new}</a></div>\
         <section class=\"karte abstand\"><h2>{shield}{setup}</h2><ul class=\"liste pruefung\">{checks}</ul>\
         <p class=\"hinweis\">{setup_hint}</p></section>",
        shield = icon("shield-check"),
        setup = ctx.say("Einrichtung", "Setup"),
        setup_hint = ctx.say(
            "Die Selbstprüfung beim Start. Was hier rot ist, zeigt die öffentliche Statusseite nur als „Einrichtung unvollständig“.",
            "The self-check at the start. What is red here, the public status page shows only as \"Setup incomplete\".",
        ),
        intro = ctx.say(
            "Hier verwaltet die Orga-Leitung Mitglieder und Aufträge. Bestand, Baupläne und Lieferungen teilen die Mitglieder aus VerseKit.",
            "Here the org's leadership manages members and orders. Members share stock, blueprints and deliveries from VerseKit.",
        ),
        emergency = if matches!(ctx.actor, Actor::Admin) && config.discord.is_some() {
            note_box(
                Tone::Warn,
                ctx.say(
                    "Notzugang: Im Alltag meldet sich die Leitung mit Discord im Portal an. Das Admin-Passwort ist für die Einrichtung und wenn Discord klemmt.",
                    "Emergency access: day to day the leadership logs in with Discord in the portal. The admin password is for the setup and when Discord fails.",
                ),
            )
        } else {
            String::new()
        },
        new_href = ctx.at("/orders/new"),
        plus = icon("plus"),
        new = ctx.say("Neuer Auftrag", "New order"),
    );
    let title = if ctx.base == ROOT { ctx.say("Verwaltung", "Administration") } else { ctx.say("Übersicht", "Overview") };
    shell(ctx, "", title, &body, 200)
}

fn members(ctx: &Ctx) -> Response {
    let config = ctx.server.config();
    let q = ctx.query("q");
    let needle = q.to_lowercase();
    let state = ctx.server.lock();
    let held = logins(&state);
    let mut list: Vec<_> = config.members.iter().collect();
    list.sort_by_key(|m| (!held.contains_key(m.id.as_str()), m.name.to_lowercase()));
    let mut rows = Vec::new();
    for member in &list {
        if !found(&needle, &[&member.name, &member.id]) {
            continue;
        }
        let rights = held.get(member.id.as_str());
        let contact = state
            .contact
            .get(&member.id)
            .map_or_else(|| ctx.say("noch nie", "never").to_string(), |at| moment(*at));
        let (state_label, class) = match rights {
            Some(_) => (ctx.say("Verbunden", "Connected"), "an"),
            None => (ctx.say("Nicht verbunden", "Not connected"), "aus"),
        };
        let rights_text = match rights {
            Some(list) if !list.is_empty() => list.iter().map(|s| scope_label(s, ctx.de)).collect::<Vec<_>>().join(", "),
            _ => "–".to_string(),
        };
        let button = if rights.is_some() {
            format!(
                "<form method=\"post\" action=\"{action}\">{hidden}\
                 <input type=\"hidden\" name=\"member\" value=\"{id}\">\
                 <button class=\"knopf klein\" type=\"submit\" aria-label=\"{aria}\">{x}{label}</button></form>",
                action = ctx.at("/members/disconnect"),
                hidden = ctx.hidden(),
                id = escape_html(&member.id),
                aria = escape_html(&ctx.say("Verbindung von {n} trennen", "Disconnect {n}").replace("{n}", &member.name)),
                x = icon("circle-x"),
                label = ctx.say("Verbindung trennen", "Disconnect"),
            )
        } else {
            String::new()
        };
        rows.push(format!(
            "<li><div class=\"zeile-kopf\"><span class=\"titel\">{name}</span>{badge}</div>\
             <dl class=\"paare\"><dt>{id_label}</dt><dd>{id}</dd><dt>{rights_label}</dt><dd>{rights_text}</dd>\
             <dt>{contact_label}</dt><dd>{contact}</dd></dl>{button}</li>",
            name = escape_html(&member.name),
            badge = badge(state_label, class),
            id_label = ctx.say("Kennung", "Id"),
            id = escape_html(&member.id),
            rights_label = ctx.say("Rechte", "Rights"),
            rights_text = escape_html(&rights_text),
            contact_label = ctx.say("Letzter Kontakt", "Last contact"),
        ));
    }
    drop(state);
    let gone = ctx.query("getrennt");
    let note = config
        .members
        .iter()
        .find(|m| m.id == gone)
        .map(|m| {
            note_box(
                Tone::Good,
                &ctx.say(
                    "Verbindung von {n} getrennt. Mit dem persönlichen Schlüssel kann sich das Mitglied neu anmelden.",
                    "{n} is disconnected. With the personal key the member can log in again.",
                )
                .replace("{n}", &m.name),
            )
        })
        .unwrap_or_default();
    let body = format!(
        "{note}{search}{count}{card}<p class=\"hinweis\">{hint}</p>",
        search = search_form(ctx, &ctx.at("/members"), &q, "", !q.is_empty(), ctx.say("Name oder Kennung", "Name or id")),
        count = count_line(ctx, rows.len(), config.members.len()),
        card = list_card("users", ctx.say("Mitglieder", "Members"), &rows, ctx.say("Kein Mitglied passt.", "No member matches.")),
        hint = ctx.say(
            "Mitglieder trägt die Orga in config.json ein. Wer dort fehlt, kann sich nicht mehr anmelden.",
            "The org lists its members in config.json. Whoever is missing there can no longer log in.",
        ),
    );
    shell(ctx, "/members", ctx.say("Mitglieder", "Members"), &body, 200)
}

fn disconnect(ctx: &Ctx) -> Response {
    let form = ctx.request.form();
    let id = value(&form, "member").unwrap_or_default().to_string();
    let Some(member) = ctx.server.config().members.iter().find(|m| m.id == id) else {
        return shell(ctx, "/members", ctx.say("Mitglieder", "Members"), &missing(ctx), 404);
    };
    let mut state = ctx.server.lock();
    state.refresh.retain(|_, g| g.member != member.id);
    state.access.retain(|_, g| g.member != member.id);
    ctx.server.save(&state);
    drop(state);
    redirect(&format!("{}?getrennt={}", ctx.at("/members"), percent_encode(&member.id)))
}

pub(crate) fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

fn stock(ctx: &Ctx) -> Response {
    let q = ctx.query("q");
    let needle = q.to_lowercase();
    let state = ctx.server.lock();
    let mut entries: Vec<(String, &Value)> = Vec::new();
    for (member, list) in &state.stock {
        for item in list {
            entries.push((ctx.server.member_name(member), item));
        }
    }
    entries.sort_by(|a, b| text(a.1, "item").to_lowercase().cmp(&text(b.1, "item").to_lowercase()).then(a.0.cmp(&b.0)));
    let total = entries.len();
    let rows: Vec<String> = entries
        .iter()
        .filter(|(member, item)| found(&needle, &[text(item, "item"), text(item, "location"), member]))
        .map(|(member, item)| {
            let mut meta = vec![format!(
                "<span>{}{} {}</span>",
                icon("package"),
                number(amount(item, "quantity"), ctx.de),
                escape_html(&unit_label(text(item, "unit"), ctx.de))
            )];
            if let Some(quality) = item.get("quality").and_then(Value::as_u64) {
                meta.push(format!("<span>{} {quality}</span>", ctx.say("Qualität", "Quality")));
            }
            if !text(item, "location").is_empty() {
                meta.push(format!("<span>{}{}</span>", icon("map-pin"), escape_html(text(item, "location"))));
            }
            meta.push(format!("<span>{}{}</span>", icon("user"), escape_html(member)));
            format!("<li><span class=\"titel\">{}</span><div class=\"meta\">{}</div></li>", escape_html(text(item, "item")), meta.concat())
        })
        .collect();
    drop(state);
    let body = format!(
        "{search}{count}{card}",
        search = search_form(ctx, &ctx.at("/stock"), &q, "", !q.is_empty(), ctx.say("Ware, Ort oder Mitglied", "Item, place or member")),
        count = count_line(ctx, rows.len(), total),
        card = list_card("warehouse",
            ctx.say("Bestand der Orga", "The org's stock"),
            &rows,
            ctx.say("Nichts gefunden. Den Bestand teilen die Mitglieder aus VerseKit.", "Nothing found. Members share their stock from VerseKit."),
        ),
    );
    shell(ctx, "/stock", ctx.say("Bestand", "Stock"), &body, 200)
}

fn blueprints(ctx: &Ctx) -> Response {
    let q = ctx.query("q");
    let needle = q.to_lowercase();
    let state = ctx.server.lock();
    let mut holders: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for (member, names) in &state.blueprints {
        for name in names {
            holders.entry(name.clone()).or_default().push(ctx.server.member_name(member));
        }
    }
    drop(state);
    let total = holders.len();
    let rows: Vec<String> = holders
        .iter()
        .filter(|(name, who)| found(&needle, &[name.as_str()]) || who.iter().any(|w| found(&needle, &[w.as_str()])))
        .map(|(name, who)| {
            format!(
                "<li><span class=\"titel\">{}</span><div class=\"meta\"><span>{}{}</span></div></li>",
                escape_html(name),
                icon("users"),
                escape_html(&who.join(", "))
            )
        })
        .collect();
    let body = format!(
        "{search}{count}{card}",
        search = search_form(ctx, &ctx.at("/blueprints"), &q, "", !q.is_empty(), ctx.say("Bauplan oder Mitglied", "Blueprint or member")),
        count = count_line(ctx, rows.len(), total),
        card = list_card("hammer",
            ctx.say("Baupläne der Orga", "The org's blueprints"),
            &rows,
            ctx.say("Nichts gefunden. Baupläne teilen die Mitglieder aus VerseKit.", "Nothing found. Members share blueprints from VerseKit."),
        ),
    );
    shell(ctx, "/blueprints", ctx.say("Baupläne", "Blueprints"), &body, 200)
}

fn order_meta(ctx: &Ctx, order: &Value) -> String {
    let (label, class) = status_label(text(order, "status"), ctx.de);
    let mut meta = vec![badge(label, class)];
    meta.push(format!("<span>{} %</span>", (progress(order) * 100.0).round()));
    if !text(order, "due").is_empty() {
        meta.push(format!("<span>{}{} {}</span>", icon("timer"), ctx.say("Frist", "Due"), escape_html(text(order, "due"))));
    }
    if !text(order, "location").is_empty() {
        meta.push(format!("<span>{}{}</span>", icon("map-pin"), escape_html(text(order, "location"))));
    }
    meta.concat()
}

fn orders(ctx: &Ctx) -> Response {
    let q = ctx.query("q");
    let needle = q.to_lowercase();
    let wanted = match ctx.query("status").as_str() {
        "done" => "done",
        "cancelled" => "cancelled",
        "all" => "all",
        _ => "open",
    };
    let state = ctx.server.lock();
    let total = state.orders.len();
    let rows: Vec<String> = state
        .orders
        .iter()
        .rev()
        .filter(|o| wanted == "all" || text(o, "status") == wanted)
        .filter(|o| {
            let items: Vec<&str> = lines_of(o).iter().map(|l| text(l, "item")).collect();
            found(&needle, &[text(o, "title"), text(o, "location"), text(o, "note")]) || items.iter().any(|i| found(&needle, &[i]))
        })
        .map(|o| {
            format!(
                "<li><a class=\"titel\" href=\"{view}?id={id}\">{title}</a>{bar}<div class=\"meta\">{meta}</div></li>",
                view = ctx.at("/orders/view"),
                id = percent_encode(text(o, "id")),
                title = escape_html(text(o, "title")),
                bar = bar(progress(o)),
                meta = order_meta(ctx, o),
            )
        })
        .collect();
    drop(state);
    let options: String = [
        ("open", ctx.say("Offen", "Open")),
        ("done", ctx.say("Erledigt", "Done")),
        ("cancelled", ctx.say("Geschlossen", "Closed")),
        ("all", ctx.say("Alle", "All")),
    ]
    .iter()
    .map(|(v, l)| format!("<option value=\"{v}\"{}>{l}</option>", if *v == wanted { " selected" } else { "" }))
    .collect();
    let select =
        format!("<label class=\"unsichtbar\" for=\"s\">{}</label><select id=\"s\" name=\"status\">{options}</select>", ctx.say("Zustand", "State"));
    let body = format!(
        "{new}{search}{count}{card}",
        new = if ctx.role >= Role::Logistics {
            format!(
                "<div class=\"knopfreihe oben\"><a class=\"knopf voll\" href=\"{}\">{}{}</a></div>",
                ctx.at("/orders/new"),
                icon("plus"),
                ctx.say("Neuer Auftrag", "New order")
            )
        } else {
            String::new()
        },
        search = search_form(ctx, &ctx.at("/orders"),
            &q,
            &select,
            !q.is_empty() || wanted != "open",
            ctx.say("Titel, Ware oder Ort", "Title, item or place")
        ),
        count = count_line(ctx, rows.len(), total),
        card = list_card("clipboard-list", ctx.say("Aufträge", "Orders"), &rows, ctx.say("Kein Auftrag passt.", "No order matches.")),
    );
    shell(ctx, "/orders", ctx.say("Aufträge", "Orders"), &body, 200)
}

fn delivery_items(ctx: &Ctx, entry: &Value) -> String {
    entry
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|i| format!("{} {}", number(amount(i, "quantity"), ctx.de), text(i, "item")))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn order_view(ctx: &Ctx) -> Response {
    let id = ctx.query("id");
    let state = ctx.server.lock();
    let Some(order) = state.orders.iter().find(|o| text(o, "id") == id).cloned() else {
        drop(state);
        return shell(ctx, "/orders", ctx.say("Auftrag", "Order"), &missing(ctx), 404);
    };
    let booked: Vec<Value> = state.delivery_log.iter().rev().filter(|d| text(d, "order") == id).cloned().collect();
    drop(state);
    let note = if !ctx.query("gespeichert").is_empty() {
        note_box(Tone::Good, ctx.say("Gespeichert.", "Saved."))
    } else if !ctx.query("geschlossen").is_empty() {
        note_box(
            Tone::Good,
            ctx.say("Geschlossen. Mitglieder können nichts mehr dazu liefern.", "Closed. Members can no longer deliver to it."),
        )
    } else if !ctx.query("offen").is_empty() {
        note_box(Tone::Good, ctx.say("Wieder offen.", "Open again."))
    } else {
        String::new()
    };
    let goods: String = lines_of(&order)
        .iter()
        .map(|l| {
            let wanted = amount(l, "quantity");
            let got = amount(l, "delivered");
            let unit = unit_label(text(l, "unit"), ctx.de);
            let share = if wanted > 0.0 { got / wanted } else { 1.0 };
            format!(
                "<li><div class=\"zeile-kopf\"><span class=\"titel\">{item}</span><span>{got} / {wanted} {unit}</span></div>{bar}</li>",
                item = escape_html(text(l, "item")),
                got = number(got, ctx.de),
                wanted = number(wanted, ctx.de),
                unit = escape_html(&unit),
                bar = bar(share),
            )
        })
        .collect();
    let (status, class) = status_label(text(&order, "status"), ctx.de);
    let mut facts = format!("<dt>{}</dt><dd>{}</dd>", ctx.say("Zustand", "State"), badge(status, class));
    facts.push_str(&format!("<dt>{}</dt><dd>{} %</dd>", ctx.say("Geliefert", "Delivered"), (progress(&order) * 100.0).round()));
    for (key, de, en) in [("location", "Ort", "Place"), ("due", "Frist", "Due")] {
        if !text(&order, key).is_empty() {
            facts.push_str(&format!("<dt>{}</dt><dd>{}</dd>", ctx.say(de, en), escape_html(text(&order, key))));
        }
    }
    if !text(&order, "note").is_empty() {
        facts.push_str(&format!("<dt>{}</dt><dd class=\"notiz\">{}</dd>", ctx.say("Notiz", "Note"), escape_html(text(&order, "note"))));
    }
    let deliveries: Vec<String> = booked.iter().map(|d| delivery_row(ctx, d, false)).collect();
    let order_id = escape_html(text(&order, "id"));
    let action = |path: &str, symbol: &str, label: &str, class: &str| {
        format!(
            "<form method=\"post\" action=\"{path}\">{hidden}<input type=\"hidden\" name=\"id\" value=\"{order_id}\">\
             <button class=\"knopf{class}\" type=\"submit\">{icon}{label}</button></form>",
            path = ctx.at(path),
            hidden = ctx.hidden(),
            icon = icon(symbol),
        )
    };
    let mut buttons = String::new();
    let status_code = text(&order, "status");
    if ctx.role >= Role::Logistics {
        if status_code != "cancelled" {
            buttons.push_str(&format!(
                "<a class=\"knopf voll\" href=\"{}?id={}\">{}{}</a>",
                ctx.at("/orders/edit"),
                percent_encode(text(&order, "id")),
                icon("pencil"),
                ctx.say("Bearbeiten", "Edit")
            ));
        }
        if status_code == "open" {
            buttons.push_str(&action("/orders/close", "ban", ctx.say("Schließen", "Close"), ""));
        } else {
            buttons.push_str(&action("/orders/reopen", "rotate-ccw", ctx.say("Wieder öffnen", "Reopen"), ""));
        }
    }
    let note = if ctx.query("bestaetigt").is_empty() {
        note
    } else {
        note_box(Tone::Good, ctx.say("Lieferung bestätigt.", "Delivery confirmed."))
    };
    let body = format!(
        "{note}<section class=\"karte\"><h2>{clip}{head}</h2><dl class=\"werte\">{facts}</dl></section>\
         <section class=\"karte\"><h2>{package}{goods_head}</h2><ul class=\"zeilen\">{goods}</ul></section>\
         {deliveries}<div class=\"knopfreihe\">{buttons}<a class=\"knopf\" href=\"{all}\">{back}{list}</a></div>",
        all = ctx.at("/orders"),
        clip = icon("clipboard-list"),
        head = ctx.say("Auftrag", "Order"),
        package = icon("package"),
        goods_head = ctx.say("Waren", "Goods"),
        deliveries = list_card("package-plus",
            ctx.say("Gebuchte Lieferungen", "Deliveries booked"),
            &deliveries,
            ctx.say("Noch keine Lieferung.", "No delivery yet."),
        ),
        back = icon("arrow-left"),
        list = ctx.say("Alle Aufträge", "All orders"),
    );
    shell(ctx, "/orders", text(&order, "title"), &body, 200)
}

/// An order as typed into the form.
#[derive(Clone, Debug, Default)]
struct Draft {
    title: String,
    location: String,
    due: String,
    note: String,
    /// Good or part, quantity, unit.
    lines: Vec<[String; 3]>,
}

/// An order after the check.
struct Checked {
    title: String,
    location: String,
    due: String,
    note: String,
    lines: Vec<(String, f64, String)>,
}

impl Draft {
    fn from_form(form: &[(String, String)]) -> Draft {
        let all = |name: &str| form.iter().filter(|(n, _)| n == name).map(|(_, v)| v.clone()).collect::<Vec<_>>();
        let (items, quantities, units) = (all("item"), all("quantity"), all("unit"));
        let rows = items.len().max(quantities.len()).max(units.len());
        let at = |list: &[String], i: usize| list.get(i).cloned().unwrap_or_default();
        Draft {
            title: value(form, "title").unwrap_or_default().to_string(),
            location: value(form, "location").unwrap_or_default().to_string(),
            due: value(form, "due").unwrap_or_default().to_string(),
            note: value(form, "note").unwrap_or_default().to_string(),
            lines: (0..rows).map(|i| [at(&items, i), at(&quantities, i), at(&units, i)]).collect(),
        }
    }

    fn from_order(order: &Value, de: bool) -> Draft {
        Draft {
            title: text(order, "title").to_string(),
            location: text(order, "location").to_string(),
            due: text(order, "due").to_string(),
            note: text(order, "note").to_string(),
            lines: lines_of(order)
                .iter()
                .map(|l| [text(l, "item").to_string(), number(amount(l, "quantity"), de), text(l, "unit").to_string()])
                .collect(),
        }
    }

    fn filled(&self) -> Vec<&[String; 3]> {
        self.lines.iter().filter(|l| !(l[0].trim().is_empty() && l[1].trim().is_empty())).collect()
    }

    fn check(&self, de: bool) -> Result<Checked, String> {
        if self.lines.len() > MAX_ROWS {
            return Err(pick(de, "Zu viele Zeilen.", "Too many rows.").into());
        }
        let mut lines: Vec<(String, f64, String)> = Vec::new();
        for (row, line) in self.lines.iter().enumerate() {
            let n = (row + 1).to_string();
            let item = line[0].trim();
            let quantity = line[1].trim();
            if item.is_empty() && quantity.is_empty() {
                continue;
            }
            if item.is_empty() {
                return Err(pick(de, "Zeile {n}: Ware oder Teil fehlt.", "Row {n}: the good or part is missing.").replace("{n}", &n));
            }
            let item = clean(item, MAX_TEXT, false, pick(de, "Ware oder Teil", "Good or part"), de)?;
            let amount = quantity
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|q| q.is_finite() && *q > 0.0 && *q <= 1e9)
                .ok_or_else(|| {
                    pick(de, "Zeile {n}: Die Menge muss eine Zahl über 0 sein.", "Row {n}: the quantity must be a number above 0.")
                        .replace("{n}", &n)
                })?;
            let unit = line[2].trim();
            if !matches!(unit, "" | "scu" | "cscu") {
                return Err(pick(de, "Zeile {n}: unbekannte Einheit.", "Row {n}: unknown unit.").replace("{n}", &n));
            }
            if lines.iter().any(|(i, _, _)| i.eq_ignore_ascii_case(&item)) {
                return Err(pick(
                    de,
                    "{i} steht zweimal im Auftrag. Fasse die Zeilen zusammen.",
                    "{i} is in the order twice. Put the rows together.",
                )
                .replace("{i}", &item));
            }
            lines.push((item, amount, unit.to_string()));
        }
        if lines.is_empty() {
            return Err(pick(de, "Der Auftrag braucht mindestens eine Ware oder ein Teil.", "The order needs at least one good or part.").into());
        }
        if lines.len() > MAX_LINES {
            return Err(pick(de, "Höchstens 50 Waren je Auftrag.", "At most 50 goods per order.").into());
        }
        let title = clean(&self.title, MAX_TEXT, false, pick(de, "Titel", "Title"), de)?;
        let title = if title.is_empty() { lines[0].0.clone() } else { title };
        let location = clean(&self.location, MAX_TEXT, false, pick(de, "Ort", "Place"), de)?;
        let note = clean(&self.note, MAX_NOTE, true, pick(de, "Notiz", "Note"), de)?;
        let due = self.due.trim().to_string();
        if !due.is_empty() && !is_date(&due) {
            return Err(pick(de, "Die Frist muss ein Datum sein (JJJJ-MM-TT).", "The due date must be a date (YYYY-MM-DD).").into());
        }
        Ok(Checked { title, location, due, note, lines })
    }
}

/// `text` trimmed, without control characters (a note keeps its line breaks), at most `max` characters.
fn clean(text: &str, max: usize, multiline: bool, field: &str, de: bool) -> Result<String, String> {
    let text = text.replace("\r\n", "\n");
    let text = text.trim();
    if text.chars().any(|c| c.is_control() && !(multiline && (c == '\n' || c == '\t'))) {
        return Err(pick(de, "{f} enthält Steuerzeichen.", "{f} holds control characters.").replace("{f}", field));
    }
    if text.chars().count() > max {
        return Err(pick(de, "{f} ist zu lang (höchstens {m} Zeichen).", "{f} is too long (at most {m} characters).")
            .replace("{f}", field)
            .replace("{m}", &max.to_string()));
    }
    Ok(text.to_string())
}

/// Whether `text` is a date `YYYY-MM-DD` of the calendar.
fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    let [year, month, day] = parts[..] else { return false };
    if year.len() != 4 || month.len() != 2 || day.len() != 2 || !text.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) else { return false };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (2000..=2999).contains(&year) && (1..=days).contains(&day)
}

fn order_form(ctx: &Ctx, id: Option<&str>, draft: &Draft, note: Option<&str>, status: u16) -> Response {
    let filled = draft.filled();
    let count = (filled.len() + EMPTY_ROWS).clamp(MIN_ROWS, MAX_LINES).max(filled.len());
    let empty = [String::new(), String::new(), String::new()];
    let rows: String = (0..count)
        .map(|i| {
            let line = filled.get(i).copied().unwrap_or(&empty);
            let n = i + 1;
            let options: String = [("", ctx.say("Stück", "Pieces")), ("scu", "SCU"), ("cscu", "cSCU")]
                .iter()
                .map(|(v, l)| format!("<option value=\"{v}\"{}>{l}</option>", if line[2] == *v { " selected" } else { "" }))
                .collect();
            format!(
                "<div class=\"reihe\">\
                 <input type=\"text\" name=\"item\" value=\"{item}\" maxlength=\"200\" placeholder=\"{item_label}\" aria-label=\"{item_label} {n}\">\
                 <input type=\"text\" name=\"quantity\" value=\"{quantity}\" inputmode=\"decimal\" placeholder=\"{qty_label}\" aria-label=\"{qty_label} {n}\">\
                 <select name=\"unit\" aria-label=\"{unit_label} {n}\">{options}</select></div>",
                item = escape_html(&line[0]),
                item_label = ctx.say("Ware oder Teil", "Good or part"),
                quantity = escape_html(&line[1]),
                qty_label = ctx.say("Menge", "Quantity"),
                unit_label = ctx.say("Einheit", "Unit"),
            )
        })
        .collect();
    let (action, title) = match id {
        Some(_) => (ctx.at("/orders/edit"), ctx.say("Auftrag bearbeiten", "Edit order")),
        None => (ctx.at("/orders/new"), ctx.say("Neuer Auftrag", "New order")),
    };
    let hidden_id = id.map(|i| format!("<input type=\"hidden\" name=\"id\" value=\"{}\">", escape_html(i))).unwrap_or_default();
    let back = id.map_or_else(|| ctx.at("/orders"), |i| format!("{}?id={}", ctx.at("/orders/view"), percent_encode(i)));
    let body = format!(
        "{note}<section class=\"karte\"><h2>{clip}{head}</h2>\
         <form method=\"post\" action=\"{action}\">{hidden}{hidden_id}\
         <label class=\"feld\" for=\"t\">{title_label}</label>\
         <input id=\"t\" type=\"text\" name=\"title\" value=\"{title_value}\" maxlength=\"200\">\
         <p class=\"klein\">{title_hint}</p>\
         <fieldset class=\"waren\"><legend>{goods}</legend>{rows}<p class=\"klein\">{rows_hint}</p></fieldset>\
         <label class=\"feld\" for=\"o\">{place}</label>\
         <input id=\"o\" type=\"text\" name=\"location\" value=\"{location}\" maxlength=\"200\" placeholder=\"{place_example}\">\
         <label class=\"feld\" for=\"f\">{due}</label>\
         <input id=\"f\" type=\"date\" name=\"due\" value=\"{due_value}\">\
         <label class=\"feld\" for=\"n\">{note_label}</label>\
         <textarea id=\"n\" name=\"note\" rows=\"4\" maxlength=\"{MAX_NOTE}\">{note_value}</textarea>\
         <div class=\"knopfreihe\"><button class=\"knopf voll\" type=\"submit\">{save}{save_label}</button>\
         <a class=\"knopf\" href=\"{back}\">{cancel}</a></div></form></section>",
        note = note.map(|text| note_box(Tone::Warn, text)).unwrap_or_default(),
        clip = icon("clipboard-list"),
        head = ctx.say("Auftrag", "Order"),
        hidden = ctx.hidden(),
        title_label = ctx.say("Titel", "Title"),
        title_value = escape_html(&draft.title),
        title_hint = ctx.say("Leer lassen: der Auftrag heißt wie die erste Ware.", "Leave empty: the order is named after the first good."),
        goods = ctx.say("Waren oder Teile", "Goods or parts"),
        rows_hint = ctx.say(
            "Leere Zeilen zählen nicht. Für mehr Zeilen speichern und wieder bearbeiten.",
            "Empty rows do not count. For more rows, save and edit again.",
        ),
        place = ctx.say("Ort", "Place"),
        location = escape_html(&draft.location),
        place_example = ctx.say("z. B. Area18, Hangar 3", "e.g. Area18, hangar 3"),
        due = ctx.say("Frist", "Due date"),
        due_value = escape_html(&draft.due),
        note_label = ctx.say("Notiz", "Note"),
        note_value = escape_html(&draft.note),
        save = icon("save"),
        save_label = ctx.say("Speichern", "Save"),
        cancel = ctx.say("Abbrechen", "Cancel"),
    );
    shell(ctx, "/orders", title, &body, status)
}

fn edit_form(ctx: &Ctx) -> Response {
    let id = ctx.query("id");
    let order = ctx.server.lock().orders.iter().find(|o| text(o, "id") == id).cloned();
    match order {
        Some(order) => order_form(ctx, Some(&id), &Draft::from_order(&order, ctx.de), None, 200),
        None => shell(ctx, "/orders", ctx.say("Auftrag", "Order"), &missing(ctx), 404),
    }
}

fn save_order(ctx: &Ctx, edit: bool) -> Response {
    let form = ctx.request.form();
    let draft = Draft::from_form(&form);
    let id = edit.then(|| value(&form, "id").unwrap_or_default().to_string());
    let checked = match draft.check(ctx.de) {
        Ok(checked) => checked,
        Err(error) => return order_form(ctx, id.as_deref(), &draft, Some(&error), 422),
    };
    let mut state = ctx.server.lock();
    let id = match id {
        None => {
            let id = loop {
                let candidate = format!("a-{}", ctx.server.random_hex(4));
                if !state.orders.iter().any(|o| text(o, "id") == candidate) {
                    break candidate;
                }
            };
            let items: Vec<Value> = checked
                .lines
                .iter()
                .map(|(item, quantity, unit)| line_json(item, *quantity, unit, 0.0))
                .collect();
            let mut order = json!({ "id": id, "title": checked.title, "status": "open", "items": items, "created": ctx.now });
            extras(&mut order, &checked);
            state.orders.push(order);
            id
        }
        Some(id) => {
            let Some(order) = state.orders.iter_mut().find(|o| text(o, "id") == id) else {
                drop(state);
                return shell(ctx, "/orders", ctx.say("Auftrag", "Order"), &missing(ctx), 404);
            };
            if text(order, "status") == "cancelled" {
                let closed = ctx.say(
                    "Ein geschlossener Auftrag lässt sich nicht bearbeiten. Öffne ihn zuerst wieder.",
                    "A closed order cannot be edited. Reopen it first.",
                );
                drop(state);
                return order_form(ctx, Some(&id), &draft, Some(closed), 409);
            }
            let old = lines_of(order).to_vec();
            if let Some(kept) = old.iter().find(|l| {
                amount(l, "delivered") > 0.0 && !checked.lines.iter().any(|(i, _, _)| i == text(l, "item"))
            }) {
                let error = ctx
                    .say("{i} hat schon Lieferungen und bleibt im Auftrag.", "{i} already has deliveries and stays in the order.")
                    .replace("{i}", text(kept, "item"));
                drop(state);
                return order_form(ctx, Some(&id), &draft, Some(&error), 422);
            }
            let items: Vec<Value> = checked
                .lines
                .iter()
                .map(|(item, quantity, unit)| {
                    let delivered = old.iter().find(|l| text(l, "item") == item).map_or(0.0, |l| amount(l, "delivered"));
                    line_json(item, *quantity, unit, delivered)
                })
                .collect();
            order["title"] = json!(checked.title);
            order["items"] = json!(items);
            for key in ["location", "due", "note"] {
                if let Some(object) = order.as_object_mut() {
                    object.remove(key);
                }
            }
            extras(order, &checked);
            let complete = progress(order) >= 1.0;
            order["status"] = json!(if complete { "done" } else { "open" });
            id
        }
    };
    ctx.server.save(&state);
    drop(state);
    redirect(&format!("{}?id={}&gespeichert=1", ctx.at("/orders/view"), percent_encode(&id)))
}

fn line_json(item: &str, quantity: f64, unit: &str, delivered: f64) -> Value {
    let mut line = json!({ "item": item, "quantity": quantity, "delivered": delivered });
    if !unit.is_empty() {
        line["unit"] = json!(unit);
    }
    line
}

fn extras(order: &mut Value, checked: &Checked) {
    for (key, field) in [("location", &checked.location), ("due", &checked.due), ("note", &checked.note)] {
        if !field.is_empty() {
            order[key] = json!(field);
        }
    }
}

fn set_status(ctx: &Ctx, reopen: bool) -> Response {
    let form = ctx.request.form();
    let id = value(&form, "id").unwrap_or_default().to_string();
    let mut state = ctx.server.lock();
    let Some(order) = state.orders.iter_mut().find(|o| text(o, "id") == id) else {
        drop(state);
        return shell(ctx, "/orders", ctx.say("Auftrag", "Order"), &missing(ctx), 404);
    };
    let (status, flag) = if reopen {
        (if progress(order) >= 1.0 { "done" } else { "open" }, "offen")
    } else if text(order, "status") == "open" {
        ("cancelled", "geschlossen")
    } else {
        let current = text(order, "status").to_string();
        (if current == "done" { "done" } else { "cancelled" }, "geschlossen")
    };
    order["status"] = json!(status);
    ctx.server.save(&state);
    drop(state);
    redirect(&format!("{}?id={}&{flag}=1", ctx.at("/orders/view"), percent_encode(&id)))
}

fn deliveries(ctx: &Ctx) -> Response {
    let q = ctx.query("q");
    let needle = q.to_lowercase();
    let wanted = match ctx.query("status").as_str() {
        "open" => "open",
        "confirmed" => "confirmed",
        _ => "all",
    };
    let state = ctx.server.lock();
    let total = state.delivery_log.len();
    let rows: Vec<String> = state
        .delivery_log
        .iter()
        .rev()
        .filter(|d| match wanted {
            "open" => d.get("confirmed").is_none(),
            "confirmed" => d.get("confirmed").is_some(),
            _ => true,
        })
        .filter(|d| found(&needle, &[text(d, "title"), &ctx.server.member_name(text(d, "member")), &delivery_items(ctx, d)]))
        .map(|d| delivery_row(ctx, d, true))
        .collect();
    drop(state);
    let options: String = [
        ("all", ctx.say("Alle", "All")),
        ("open", ctx.say("Unbestätigt", "Unconfirmed")),
        ("confirmed", ctx.say("Bestätigt", "Confirmed")),
    ]
    .iter()
    .map(|(v, l)| format!("<option value=\"{v}\"{}>{l}</option>", if *v == wanted { " selected" } else { "" }))
    .collect();
    let select =
        format!("<label class=\"unsichtbar\" for=\"s\">{}</label><select id=\"s\" name=\"status\">{options}</select>", ctx.say("Zustand", "State"));
    let note = if ctx.query("bestaetigt").is_empty() {
        String::new()
    } else {
        note_box(Tone::Good, ctx.say("Lieferung bestätigt.", "Delivery confirmed."))
    };
    let body = format!(
        "{note}{search}{count}{card}",
        search = search_form(
            ctx,
            &ctx.at("/deliveries"),
            &q,
            &select,
            !q.is_empty() || wanted != "all",
            ctx.say("Auftrag, Ware oder Mitglied", "Order, item or member")
        ),
        count = count_line(ctx, rows.len(), total),
        card = list_card("package-plus",
            ctx.say("Lieferungen", "Deliveries"),
            &rows,
            ctx.say("Nichts gefunden. Lieferungen buchen die Mitglieder aus VerseKit.", "Nothing found. Members book deliveries from VerseKit."),
        ),
    );
    shell(ctx, "/deliveries", ctx.say("Lieferungen", "Deliveries"), &body, 200)
}

/// One delivery of the log: the goods, who booked it and when, whether and by whom it was confirmed, and for the
/// role logistics the button that confirms it. `listed` links the title to its order (the list of all deliveries).
fn delivery_row(ctx: &Ctx, d: &Value, listed: bool) -> String {
    let head = if listed {
        format!(
            "<a class=\"titel\" href=\"{view}?id={id}\">{title}</a>",
            view = ctx.at("/orders/view"),
            id = percent_encode(text(d, "order")),
            title = escape_html(text(d, "title")),
        )
    } else {
        format!("<span class=\"titel\">{}</span>", escape_html(&delivery_items(ctx, d)))
    };
    let goods = if listed { format!("<span>{}{}</span>", icon("package"), escape_html(&delivery_items(ctx, d))) } else { String::new() };
    let (mark, button) = match d.get("confirmed") {
        Some(confirmed) => {
            let label = ctx.say("Bestätigt von {n}", "Confirmed by {n}").replace("{n}", text(confirmed, "by"));
            (badge(&label, "an"), String::new())
        }
        None => {
            let button = if ctx.role >= Role::Logistics {
                format!(
                    "<form method=\"post\" action=\"{action}\">{hidden}<input type=\"hidden\" name=\"id\" value=\"{id}\">\
                     <input type=\"hidden\" name=\"back\" value=\"{back}\">\
                     <button class=\"knopf klein\" type=\"submit\">{icon}{label}</button></form>",
                    action = ctx.at("/deliveries/confirm"),
                    hidden = ctx.hidden(),
                    id = escape_html(text(d, "id")),
                    back = if listed { "list" } else { "order" },
                    icon = icon("check"),
                    label = ctx.say("Lieferung bestätigen", "Confirm delivery"),
                )
            } else {
                String::new()
            };
            (badge(ctx.say("Unbestätigt", "Unconfirmed"), "aus"), button)
        }
    };
    format!(
        "<li><div class=\"zeile-kopf\">{head}{mark}</div>\
         <div class=\"meta\">{goods}<span>{user}{member}</span><span>{clock}{at}</span></div>{button}</li>",
        user = icon("user"),
        member = escape_html(&ctx.server.member_name(text(d, "member"))),
        clock = icon("clock"),
        at = moment(d.get("at").and_then(Value::as_u64).unwrap_or(0)),
    )
}

/// Marks a delivery of the log as confirmed by the actor; a delivery confirmed already keeps its first confirmation.
fn confirm_delivery(ctx: &Ctx) -> Response {
    let form = ctx.request.form();
    let id = value(&form, "id").unwrap_or_default().to_string();
    let mut state = ctx.server.lock();
    let Some(entry) = state.delivery_log.iter_mut().find(|d| !id.is_empty() && text(d, "id") == id) else {
        drop(state);
        return shell(ctx, "/deliveries", ctx.say("Lieferungen", "Deliveries"), &missing(ctx), 404);
    };
    if entry.get("confirmed").is_none() {
        entry["confirmed"] = json!({ "by": ctx.actor_name(), "at": ctx.now });
    }
    let order = text(entry, "order").to_string();
    ctx.server.save(&state);
    drop(state);
    if value(&form, "back") == Some("order") {
        redirect(&format!("{}?id={}&bestaetigt=1", ctx.at("/orders/view"), percent_encode(&order)))
    } else {
        redirect(&format!("{}?bestaetigt=1", ctx.at("/deliveries")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_numbers_and_text_are_checked() {
        assert!(is_date("2026-02-28") && is_date("2028-02-29"));
        assert!(!is_date("2026-02-29") && !is_date("2026-13-01") && !is_date("26-01-01") && !is_date("2026-1-01"));
        assert_eq!(number(4.0, true), "4");
        assert_eq!(number(1.25, true), "1,25");
        assert_eq!(number(0.5, false), "0.5");
        assert!(clean("a\u{7}b", 10, false, "x", false).is_err());
        assert_eq!(clean(" a\r\nb ", 10, true, "x", false).unwrap(), "a\nb");
        assert!(clean("abc", 2, false, "x", false).is_err());
    }

    #[test]
    fn the_admin_key_checks_and_hides_the_password() {
        assert!(AdminKey::new("kurz").is_err());
        let key = AdminKey::new("ein-langes-passwort").unwrap();
        assert!(key.verify("ein-langes-passwort"));
        assert!(!key.verify("ein-langes-passwort "));
        assert!(!format!("{key:?}").contains("passwort"));
    }

    #[test]
    fn a_form_from_another_site_is_not_the_own() {
        let own = Request::new("POST", "/admin/x", &[("Host", "orga.example.org"), ("Origin", "https://orga.example.org")], b"");
        assert!(same_origin(&own));
        let other = Request::new("POST", "/admin/x", &[("Host", "orga.example.org"), ("Origin", "https://evil.example")], b"");
        assert!(!same_origin(&other));
        let null = Request::new("POST", "/admin/x", &[("Host", "orga.example.org"), ("Origin", "null")], b"");
        assert!(!same_origin(&null));
        let site = Request::new("POST", "/admin/x", &[("Sec-Fetch-Site", "cross-site")], b"");
        assert!(!same_origin(&site));
    }
}
