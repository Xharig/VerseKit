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

//! The members' portal at `/portal`: members of the org's Discord server log in with Discord and see the pages of
//! `admin.rs` as far as their role allows. A layer of this server only, never part of the standard.
//!
//! | Part | Does |
//! |---|---|
//! | off | without the three Discord settings of the environment the portal is off and says so |
//! | login | OAuth2 authorization code with PKCE (S256) and a single-use `state` tied to the browser by a cookie; the code is exchanged with the client secret, the Discord token asks once for the member of the org's Discord server and is revoked at once; nothing of Discord is kept but id, names and role ids |
//! | roles | member, logistics, lead: from the Discord roles the lead maps in the settings, or set by hand; blocked keeps someone out whatever Discord says; without a role a login waits until the lead lets it in |
//! | session | in memory behind a cookie (`__Host-` under https, HttpOnly, SameSite=Strict, Path=/), ends after 8 hours or 2 hours unused; the role is read again on every request, so blocking acts at once |
//! | forms | every POST needs the session's CSRF token and the own origin |

use std::collections::{BTreeMap, HashMap};

use serde_json::{Value, json};

use crate::admin::{self, Actor, Ctx, badge, count_line, found, list_card, percent_encode, redirect, search_form, shell};
use crate::http::{Request, Response, escape_html, value};
use crate::look::{german, icon, page_with_head, pick};
use crate::panel::{Tone, note_box};
use crate::status::moment;
use crate::{Server, State, hash};

/// The root of the portal.
pub const ROOT: &str = "/portal";
/// The address Discord sends the browser back to.
pub const CALLBACK: &str = "/portal/callback";
/// The rights asked of Discord: who the user is, and the user's membership of the org's Discord server.
pub const SCOPE: &str = "identify guilds.members.read";
pub const AUTHORIZE_URL: &str = "https://discord.com/oauth2/authorize";
pub const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
pub const REVOKE_URL: &str = "https://discord.com/api/oauth2/token/revoke";
pub const API_URL: &str = "https://discord.com/api/v10";
/// The issuer Discord names in the redirect (RFC 9207).
pub const ISSUER: &str = "https://discord.com";
/// The longest a session lasts.
pub const SESSION_SECONDS: u64 = 8 * 3600;
/// A session not used for so long ends.
pub const IDLE_SECONDS: u64 = 2 * 3600;
/// How long a started login waits for Discord.
pub const LOGIN_SECONDS: u64 = 600;
/// Logins waiting for Discord at the same time; more are refused with 429.
pub const LOGINS_MAX: usize = 500;
/// Sessions at the same time; the oldest goes first.
pub const SESSIONS_MAX: usize = 5000;
/// Sessions of one person at the same time; the oldest goes first.
pub const SESSIONS_PER_USER: usize = 5;
/// Logins answered by Discord per minute; more wait for the next minute.
pub const CALLBACKS_PER_MINUTE: u32 = 60;
/// People the portal keeps; a new one beyond is not let in.
pub const USERS_MAX: usize = 5000;
/// Discord roles per role of the portal.
pub const ROLES_MAX: usize = 50;
const NAME_MAX: usize = 100;
const DISCORD_ROLES_MAX: usize = 250;

/// The Discord application of the org and its Discord server; `Debug` shows nothing of the secret.
#[derive(Clone)]
pub struct Discord {
    pub client_id: String,
    secret: String,
    pub guild_id: String,
}

impl std::fmt::Debug for Discord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Discord {{ client_id: {}, guild_id: {}, secret: hidden }}", self.client_id, self.guild_id)
    }
}

/// Whether `text` is a Discord id: 17 to 20 digits.
pub fn snowflake(text: &str) -> bool {
    (17..=20).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit())
}

impl Discord {
    /// The application `client_id` with its secret and the Discord server `guild_id`; the error names the value that
    /// is wrong, never the secret.
    pub fn new(client_id: &str, secret: &str, guild_id: &str) -> Result<Discord, String> {
        if !snowflake(client_id.trim()) {
            return Err("VERSEKIT_ORG_DISCORD_CLIENT_ID is no Discord id (17 to 20 digits)".into());
        }
        if !snowflake(guild_id.trim()) {
            return Err("VERSEKIT_ORG_DISCORD_GUILD_ID is no Discord id (17 to 20 digits)".into());
        }
        let secret = secret.trim();
        if secret.len() < 16 || secret.len() > 200 || !secret.bytes().all(|b| b.is_ascii_graphic()) {
            return Err("VERSEKIT_ORG_DISCORD_CLIENT_SECRET is not the client secret of the Discord application".into());
        }
        Ok(Discord { client_id: client_id.trim().into(), secret: secret.into(), guild_id: guild_id.trim().into() })
    }
}

/// The role of a person in the portal; a higher role may all of the lower ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Member,
    Logistics,
    Lead,
}

/// The name of a role on the pages.
pub(crate) fn role_label(role: Role, de: bool) -> &'static str {
    match role {
        Role::Member => pick(de, "Mitglied", "Member"),
        Role::Logistics => pick(de, "Logistik", "Logistics"),
        Role::Lead => pick(de, "Leitung", "Lead"),
    }
}

/// What the lead set by hand for a person.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Manual {
    /// The role follows the Discord roles.
    #[default]
    Auto,
    Member,
    Logistics,
    Lead,
    /// Kept out, whatever Discord says.
    Blocked,
}

impl Manual {
    const ALL: [Manual; 5] = [Manual::Auto, Manual::Member, Manual::Logistics, Manual::Lead, Manual::Blocked];

    fn code(self) -> &'static str {
        match self {
            Manual::Auto => "auto",
            Manual::Member => "member",
            Manual::Logistics => "logistics",
            Manual::Lead => "lead",
            Manual::Blocked => "blocked",
        }
    }

    fn from_code(code: &str) -> Option<Manual> {
        Manual::ALL.into_iter().find(|m| m.code() == code)
    }

    fn label(self, de: bool) -> &'static str {
        match self {
            Manual::Auto => pick(de, "Aus den Discord-Rollen", "From the Discord roles"),
            Manual::Member => pick(de, "Mitglied", "Member"),
            Manual::Logistics => pick(de, "Logistik", "Logistics"),
            Manual::Lead => pick(de, "Leitung", "Lead"),
            Manual::Blocked => pick(de, "Gesperrt", "Blocked"),
        }
    }
}

/// A person who logged in through Discord as a member of the org's Discord server.
#[derive(Clone, Debug, Default)]
pub(crate) struct PortalUser {
    /// The name shown: the nickname on the server, else the global name, else the user name.
    name: String,
    username: String,
    /// The ids of the Discord roles at the last login.
    roles: Vec<String>,
    manual: Manual,
    first: u64,
    last: u64,
}

/// Which Discord roles give which role of the portal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RoleMap {
    member: Vec<String>,
    logistics: Vec<String>,
    lead: Vec<String>,
    /// Every member of the Discord server counts as a member of the portal.
    everyone: bool,
}

impl RoleMap {
    fn is_empty(&self) -> bool {
        self.member.is_empty() && self.logistics.is_empty() && self.lead.is_empty() && !self.everyone
    }
}

/// The role a person has now; `None` keeps the person out.
fn effective(user: &PortalUser, map: &RoleMap) -> Option<Role> {
    let has = |list: &[String]| user.roles.iter().any(|r| list.contains(r));
    match user.manual {
        Manual::Blocked => None,
        Manual::Member => Some(Role::Member),
        Manual::Logistics => Some(Role::Logistics),
        Manual::Lead => Some(Role::Lead),
        Manual::Auto if has(&map.lead) => Some(Role::Lead),
        Manual::Auto if has(&map.logistics) => Some(Role::Logistics),
        Manual::Auto if has(&map.member) || map.everyone => Some(Role::Member),
        Manual::Auto => None,
    }
}

/// One session of the portal.
#[derive(Clone, Debug)]
struct PortalSession {
    user: String,
    csrf: String,
    created: u64,
    seen: u64,
}

/// A login sent to Discord: the hashes of `state` and of the cookie of the browser, and the PKCE verifier.
#[derive(Clone, Debug)]
struct LoginStart {
    state: String,
    binding: String,
    verifier: String,
    expires: u64,
}

/// What the portal keeps: the people and the role map go into the state file; sessions and logins stay in memory.
#[derive(Default)]
pub(crate) struct PortalState {
    users: BTreeMap<String, PortalUser>,
    map: RoleMap,
    sessions: HashMap<String, PortalSession>,
    logins: Vec<LoginStart>,
    /// Logins answered by Discord: minute and count.
    callbacks: (u64, u32),
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

impl PortalState {
    pub(crate) fn to_json(&self) -> Value {
        let users: serde_json::Map<String, Value> = self
            .users
            .iter()
            .map(|(id, u)| {
                (
                    id.clone(),
                    json!({ "name": u.name, "username": u.username, "roles": u.roles, "manual": u.manual.code(), "first": u.first, "last": u.last }),
                )
            })
            .collect();
        json!({
            "users": users,
            "roles": { "member": self.map.member, "logistics": self.map.logistics, "lead": self.map.lead, "everyone": self.map.everyone },
        })
    }

    pub(crate) fn from_json(value: Option<&Value>) -> PortalState {
        let mut state = PortalState::default();
        let Some(root) = value else { return state };
        if let Some(users) = root.get("users").and_then(Value::as_object) {
            for (id, u) in users.iter().filter(|(id, _)| snowflake(id)) {
                let text = |key: &str| u.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
                let number = |key: &str| u.get(key).and_then(Value::as_u64).unwrap_or(0);
                state.users.insert(
                    id.clone(),
                    PortalUser {
                        name: text("name"),
                        username: text("username"),
                        roles: strings(u.get("roles")),
                        manual: Manual::from_code(&text("manual")).unwrap_or_default(),
                        first: number("first"),
                        last: number("last"),
                    },
                );
            }
        }
        if let Some(roles) = root.get("roles") {
            state.map = RoleMap {
                member: strings(roles.get("member")),
                logistics: strings(roles.get("logistics")),
                lead: strings(roles.get("lead")),
                everyone: roles.get("everyone").and_then(Value::as_bool).unwrap_or(false),
            };
        }
        state
    }
}

/// How many people wait for the lead to let them in.
pub(crate) fn waiting(state: &State) -> usize {
    let portal = &state.portal;
    portal.users.values().filter(|u| u.manual != Manual::Blocked && effective(u, &portal.map).is_none()).count()
}

/// Whether the lead mapped no Discord role and let nobody in by hand: then every login waits.
pub(crate) fn unmapped(state: &State) -> bool {
    let portal = &state.portal;
    portal.map.is_empty() && !portal.users.values().any(|u| effective(u, &portal.map).is_some())
}

/// Base64 of `bytes`: the URL alphabet without padding, or the standard one with it.
pub fn base64(bytes: &[u8], url: bool) -> String {
    let table: &[u8; 64] = if url {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    } else {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    };
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(table[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else if !url {
                out.push('=');
            }
        }
    }
    out
}

/// The PKCE challenge of a verifier (S256).
pub fn challenge(verifier: &str) -> String {
    base64(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()).as_ref(), true)
}

/// Whether `c` turns the direction of text or is invisible: the bidi marks and embeddings, the zero-width
/// characters and the soft hyphen.
fn invisible(c: char) -> bool {
    matches!(c, '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

/// A name from Discord as shown: without control characters and without characters that turn or hide text,
/// trimmed, at most 100 characters.
fn clean_name(text: &str) -> String {
    text.chars().filter(|c| !c.is_control() && !invisible(*c)).collect::<String>().trim().chars().take(NAME_MAX).collect()
}

fn secure(server: &Server) -> bool {
    server.config().public_url.starts_with("https://")
}

/// The cookie of a session: with the prefix `__Host-` under https, so no other host and no other path can set it.
pub(crate) fn session_cookie(server: &Server) -> &'static str {
    if secure(server) { "__Host-vk_portal" } else { "vk_portal" }
}

/// The cookie that ties a login sent to Discord to the browser that started it.
pub(crate) fn login_cookie(server: &Server) -> &'static str {
    if secure(server) { "__Host-vk_portal_login" } else { "vk_portal_login" }
}

/// `strict` for a session; a login cookie must come back from Discord's redirect and is `Lax`.
fn cookie_attributes(server: &Server, strict: bool) -> String {
    let same = if strict { "Strict" } else { "Lax" };
    let secure = if secure(server) { "; Secure" } else { "" };
    format!("; Path=/; HttpOnly; SameSite={same}{secure}")
}

fn clear(server: &Server, name: &str, strict: bool) -> String {
    format!("{name}=; Max-Age=0{}", cookie_attributes(server, strict))
}

/// A page of the portal outside a session.
fn plain(server: &Server, de: bool, status: u16, body: &str) -> Response {
    let config = server.config();
    let title = pick(de, "Mitglieder-Portal", "Members' portal");
    let page = page_with_head(de, &config.name, &config.tag, title, &format!("<h1>{title}</h1>{body}"), "");
    admin::language(Response::html(status, page), de)
}

/// A message outside a session with the way back to the start of the portal.
fn message(server: &Server, de: bool, status: u16, tone: Tone, text: &str) -> Response {
    let body = format!(
        "{}<div class=\"knopfreihe\"><a class=\"knopf\" href=\"{ROOT}\">{}{}</a></div>",
        note_box(tone, text),
        icon("arrow-left"),
        pick(de, "Zum Portal", "To the portal"),
    );
    plain(server, de, status, &body)
}

fn off(server: &Server, de: bool) -> Response {
    let body = format!(
        "{note}<section class=\"karte\"><h2>{info}{head}</h2><p>{how}</p></section>\
         <div class=\"knopfreihe\"><a class=\"knopf\" href=\"/\">{back}{overview}</a></div>",
        note = note_box(Tone::Warn, pick(de, "Das Mitglieder-Portal ist aus.", "The members' portal is off.")),
        info = icon("info"),
        head = pick(de, "Für die Orga-Leitung", "For the org's leadership"),
        how = pick(
            de,
            "Das Portal braucht eine eigene Discord-App der Orga. Legt sie im Discord-Entwicklerportal an und setzt <code>VERSEKIT_ORG_DISCORD_CLIENT_ID</code>, <code>VERSEKIT_ORG_DISCORD_CLIENT_SECRET</code> und <code>VERSEKIT_ORG_DISCORD_GUILD_ID</code> in der Datei <code>.env</code>. Die Anleitung zum Selbsthosten zeigt jeden Schritt.",
            "The portal needs a Discord application of the org. Create it in the Discord developer portal and set <code>VERSEKIT_ORG_DISCORD_CLIENT_ID</code>, <code>VERSEKIT_ORG_DISCORD_CLIENT_SECRET</code> and <code>VERSEKIT_ORG_DISCORD_GUILD_ID</code> in the file <code>.env</code>. The self-hosting guide shows every step.",
        ),
        back = icon("arrow-left"),
        overview = pick(de, "Zur Übersicht", "To the overview"),
    );
    plain(server, de, 404, &body)
}

/// The start of the portal without a session: what it is and the button to Discord.
fn landing(server: &Server, de: bool, note: Option<(Tone, &str)>) -> Response {
    let body = format!(
        "<p class=\"einleitung\">{intro}</p>{note}\
         <section class=\"karte\"><h2>{lock}{head}</h2><p>{text}</p>\
         <div class=\"knopfreihe\"><a class=\"knopf voll\" href=\"/portal/login\">{login}{go}</a></div>\
         <p class=\"hinweis\">{hint}</p></section>",
        intro = pick(
            de,
            "Hier sehen die Mitglieder Bestand, Baupläne und Aufträge der Orga.",
            "Here the members see the org's stock, blueprints and orders.",
        ),
        note = note.map(|(tone, text)| note_box(tone, text)).unwrap_or_default(),
        lock = icon("lock"),
        head = pick(de, "Anmelden", "Log in"),
        text = pick(
            de,
            "Melde dich mit deinem Discord-Konto an. Hinein kommt nur, wer auf dem Discord-Server der Orga ist und eine Rolle hat.",
            "Log in with your Discord account. Only who is on the org's Discord server and holds a role gets in.",
        ),
        login = icon("log-in"),
        go = pick(de, "Mit Discord anmelden", "Log in with Discord"),
        hint = pick(
            de,
            "Discord zeigt der Orga nur deinen Namen, deine Kennung und deine Rollen auf ihrem Server. VerseKit selbst verbindest du weiter unter Einstellungen › Orga verbinden.",
            "Discord shows the org only your name, your id and your roles on its server. VerseKit itself you still connect under Settings › Connect org.",
        ),
    );
    plain(server, de, 200, &body)
}

/// Answers every request under `/portal`.
pub(crate) fn handle(server: &Server, request: &Request, now: u64) -> Response {
    let de = german(request);
    let Some(discord) = server.config().discord.clone() else { return off(server, de) };
    let path = request.path.as_str();
    let method = request.method.as_str();
    if path == "/portal/login" {
        return if method == "GET" { start(server, &discord, de, now) } else { crate::not_allowed() };
    }
    if path == CALLBACK {
        return if method == "GET" { callback(server, &discord, request, de, now) } else { crate::not_allowed() };
    }
    let token = admin::cookie(request, session_cookie(server)).map(hash);
    let found = {
        let mut state = server.lock();
        let portal = &mut state.portal;
        portal.sessions.retain(|_, s| s.created + SESSION_SECONDS > now && s.seen + IDLE_SECONDS > now);
        match token.as_ref().and_then(|t| portal.sessions.get(t).cloned()) {
            None => None,
            Some(session) => {
                let user = portal.users.get(&session.user).cloned();
                match user.as_ref().and_then(|u| effective(u, &portal.map)) {
                    Some(role) => {
                        if let Some(s) = token.as_ref().and_then(|t| portal.sessions.get_mut(t)) {
                            s.seen = now;
                        }
                        Some(Some((session, role, user.map(|u| u.name).unwrap_or_default())))
                    }
                    None => {
                        let gone = session.user.clone();
                        portal.sessions.retain(|_, s| s.user != gone);
                        Some(None)
                    }
                }
            }
        }
    };
    let (session, role, name) = match found {
        Some(Some(found)) => found,
        Some(None) => {
            let text = pick(de, "Du hast keinen Zugang mehr. Frag die Orga-Leitung.", "You no longer have access. Ask the org's leadership.");
            return message(server, de, 403, Tone::Warn, text).with_header("Set-Cookie", &clear(server, session_cookie(server), true));
        }
        None if method == "GET" && path == ROOT => {
            let out = value(&request.query_pairs(), "out").is_some();
            return landing(server, de, out.then(|| (Tone::Good, pick(de, "Abgemeldet.", "Logged out."))));
        }
        None if method == "GET" => return redirect(ROOT),
        None => return admin::refused_at(server, de, 403, pick(de, "Nicht angemeldet.", "Not logged in."), ROOT),
    };
    if method == "POST" && !admin::form_ok(request, &session.csrf) {
        return admin::refused_at(server, de, 403, admin::stale(de), ROOT);
    }
    let ctx = Ctx {
        server,
        request,
        de,
        csrf: session.csrf.clone(),
        now,
        base: ROOT,
        role,
        actor: Actor::Member { id: session.user.clone(), name },
    };
    let sub = &path[ROOT.len()..];
    if sub == "/logout" {
        if method != "POST" {
            return crate::not_allowed();
        }
        if let Some(token) = token {
            server.lock().portal.sessions.remove(&token);
        }
        return redirect("/portal?out=1").with_header("Set-Cookie", &clear(server, session_cookie(server), true));
    }
    admin::pages(&ctx, sub)
}

/// Starts a login: a fresh `state`, a cookie for this browser and a PKCE verifier, then off to Discord.
fn start(server: &Server, discord: &Discord, de: bool, now: u64) -> Response {
    let state_token = server.random_hex(32);
    let binding = server.random_hex(32);
    let verifier = base64(&server.random_bytes(32), true);
    {
        let mut state = server.lock();
        let logins = &mut state.portal.logins;
        logins.retain(|l| l.expires > now);
        if logins.len() >= LOGINS_MAX {
            let busy = pick(de, "Gerade melden sich zu viele an. Versuch es in einer Minute wieder.", "Too many logins just now. Try again in a minute.");
            return message(server, de, 429, Tone::Warn, busy).with_header("Retry-After", "60");
        }
        logins.push(LoginStart { state: hash(&state_token), binding: hash(&binding), verifier: verifier.clone(), expires: now + LOGIN_SECONDS });
    }
    let target = format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={client}&scope={scope}&state={state_token}&redirect_uri={redirect}\
         &code_challenge={challenge}&code_challenge_method=S256",
        client = percent_encode(&discord.client_id),
        scope = percent_encode(SCOPE),
        redirect = percent_encode(&redirect_uri(server)),
        challenge = challenge(&verifier),
    );
    redirect(&target).with_header(
        "Set-Cookie",
        &format!("{}={binding}; Max-Age={LOGIN_SECONDS}{}", login_cookie(server), cookie_attributes(server, false)),
    )
}

/// The address Discord sends the browser back to, as registered in the Discord application.
pub fn redirect_uri(server: &Server) -> String {
    format!("{}{CALLBACK}", server.config().public_url)
}

/// What Discord said about the person who logged in.
struct Found {
    id: String,
    name: String,
    username: String,
    roles: Vec<String>,
}

/// Why a login through Discord ended without a person.
enum Failure {
    /// Not on the org's Discord server, or its rules not yet accepted.
    NotMember,
    /// Discord asks to wait.
    Busy,
    /// Anything else: an answer that does not fit, a network failure.
    Broken(String),
}

fn basic(discord: &Discord) -> String {
    format!("Basic {}", base64(format!("{}:{}", discord.client_id, discord.secret).as_bytes(), false))
}

/// Exchanges the code with the secret and the PKCE verifier, asks for the member of the org's Discord server and
/// revokes the Discord token again.
fn ask_discord(server: &Server, discord: &Discord, code: &str, verifier: &str) -> Result<Found, Failure> {
    let outbound = server.outbound();
    let auth = basic(discord);
    let form = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&code_verifier={}",
        percent_encode(code),
        percent_encode(&redirect_uri(server)),
        percent_encode(verifier),
    );
    let headers = [("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json"), ("Authorization", auth.as_str())];
    let answer = outbound.exchange("POST", TOKEN_URL, &headers, form.as_bytes()).map_err(Failure::Broken)?;
    if answer.status == 429 {
        return Err(Failure::Busy);
    }
    if answer.status != 200 {
        return Err(Failure::Broken(format!("the token endpoint of Discord answered {}", answer.status)));
    }
    let tokens = answer.json_body();
    let given = tokens.get("access_token").and_then(Value::as_str).filter(|t| !t.is_empty() && t.len() <= 4096).map(str::to_string);
    // Revoking one token of an authorization ends all of them, the refresh token too; a failure is not told apart,
    // the token runs out at Discord by itself.
    let revoke = |token: &str| {
        let form = format!("token={}&token_type_hint=access_token", percent_encode(token));
        let headers = [("Content-Type", "application/x-www-form-urlencoded"), ("Authorization", auth.as_str())];
        let _ = outbound.exchange("POST", REVOKE_URL, &headers, form.as_bytes());
    };
    let Some(access) = given else {
        return Err(Failure::Broken("Discord sent no access token".into()));
    };
    let bearer_type = tokens.get("token_type").and_then(Value::as_str).is_some_and(|t| t.eq_ignore_ascii_case("Bearer"));
    if !bearer_type || access.len() > 512 || !access.bytes().all(|b| b.is_ascii_graphic()) {
        revoke(&access);
        return Err(Failure::Broken("Discord sent no bearer token that fits".into()));
    }
    let bearer = format!("Bearer {access}");
    let url = format!("{API_URL}/users/@me/guilds/{}/member", discord.guild_id);
    let member = outbound.exchange("GET", &url, &[("Accept", "application/json"), ("Authorization", bearer.as_str())], b"");
    revoke(&access);
    let member = member.map_err(Failure::Broken)?;
    match member.status {
        200 => {}
        403 | 404 => return Err(Failure::NotMember),
        429 => return Err(Failure::Busy),
        other => return Err(Failure::Broken(format!("Discord answered {other} for the member"))),
    }
    let body = member.json_body();
    if body.get("pending").and_then(Value::as_bool) == Some(true) {
        return Err(Failure::NotMember);
    }
    let user = body.get("user").ok_or_else(|| Failure::Broken("the member has no user".into()))?;
    let id = user.get("id").and_then(Value::as_str).filter(|id| snowflake(id)).ok_or_else(|| Failure::Broken("the user has no id".into()))?;
    let text = |object: &Value, key: &str| object.get(key).and_then(Value::as_str).map(clean_name).filter(|s| !s.is_empty());
    let username = text(user, "username").unwrap_or_else(|| id.to_string());
    let name = text(&body, "nick").or_else(|| text(user, "global_name")).unwrap_or_else(|| username.clone());
    let roles: Vec<String> = body
        .get("roles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|r| snowflake(r))
        .take(DISCORD_ROLES_MAX)
        .map(str::to_string)
        .collect();
    Ok(Found { id: id.to_string(), name, username, roles })
}

/// The way back from Discord: checks `state` against the cookie of this browser (once only), asks Discord, keeps
/// the person and opens a session when the person has a role.
fn callback(server: &Server, discord: &Discord, request: &Request, de: bool, now: u64) -> Response {
    let query = request.query_pairs();
    let clear_login = clear(server, login_cookie(server), false);
    let stale = pick(
        de,
        "Die Anmeldung ist abgelaufen oder kam nicht aus diesem Browser. Melde dich noch einmal an.",
        "The login has expired or did not come from this browser. Log in again.",
    );
    let given = value(&query, "state").filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())).map(hash);
    let binding = admin::cookie(request, login_cookie(server)).map(hash);
    let verifier = {
        let mut state = server.lock();
        let portal = &mut state.portal;
        portal.logins.retain(|l| l.expires > now);
        let index = given.as_ref().and_then(|g| portal.logins.iter().position(|l| l.state == *g));
        let Some(index) = index else {
            return message(server, de, 400, Tone::Warn, stale).with_header("Set-Cookie", &clear_login);
        };
        let login = portal.logins.remove(index);
        if binding.as_deref() != Some(login.binding.as_str()) {
            return message(server, de, 400, Tone::Warn, stale).with_header("Set-Cookie", &clear_login);
        }
        let minute = now / 60;
        portal.callbacks = if portal.callbacks.0 == minute { (minute, portal.callbacks.1 + 1) } else { (minute, 1) };
        if portal.callbacks.1 > CALLBACKS_PER_MINUTE {
            let busy = pick(de, "Gerade melden sich zu viele an. Versuch es in einer Minute wieder.", "Too many logins just now. Try again in a minute.");
            return message(server, de, 429, Tone::Warn, busy).with_header("Retry-After", "60").with_header("Set-Cookie", &clear_login);
        }
        login.verifier
    };
    if value(&query, "error").is_some() {
        let back = pick(de, "Die Anmeldung bei Discord wurde abgebrochen.", "The login at Discord was cancelled.");
        return message(server, de, 400, Tone::Warn, back).with_header("Set-Cookie", &clear_login);
    }
    // A redirect without `iss` passes: Discord is the only issuer, and state, cookie and PKCE tie the code already.
    if value(&query, "iss").is_some_and(|iss| iss != ISSUER) {
        return message(server, de, 400, Tone::Warn, stale).with_header("Set-Cookie", &clear_login);
    }
    let Some(code) = value(&query, "code").filter(|c| !c.is_empty() && c.len() <= 256 && c.bytes().all(|b| b.is_ascii_alphanumeric() || b"-._~".contains(&b)))
    else {
        return message(server, de, 400, Tone::Warn, stale).with_header("Set-Cookie", &clear_login);
    };
    let found = match ask_discord(server, discord, code, &verifier) {
        Ok(found) => found,
        Err(Failure::NotMember) => {
            let text = pick(
                de,
                "Du bist nicht auf dem Discord-Server der Orga, oder du hast seine Regeln noch nicht angenommen. Nur Mitglieder kommen hinein.",
                "You are not on the org's Discord server, or you have not accepted its rules yet. Only members get in.",
            );
            return message(server, de, 403, Tone::Warn, text).with_header("Set-Cookie", &clear_login);
        }
        Err(Failure::Busy) => {
            let text = pick(de, "Discord ist gerade überlastet. Versuch es in einer Minute wieder.", "Discord is busy just now. Try again in a minute.");
            return message(server, de, 503, Tone::Warn, text).with_header("Retry-After", "60").with_header("Set-Cookie", &clear_login);
        }
        Err(Failure::Broken(why)) => {
            eprintln!("portal: login through Discord failed: {why}");
            let text = pick(
                de,
                "Discord hat die Anmeldung nicht bestätigt. Versuch es noch einmal; klappt es wieder nicht, sieht die Orga-Leitung im Log nach.",
                "Discord did not confirm the login. Try again; if it fails again, the org's leadership looks into the log.",
            );
            return message(server, de, 502, Tone::Warn, text).with_header("Set-Cookie", &clear_login);
        }
    };
    let mut state = server.lock();
    let portal = &mut state.portal;
    if !portal.users.contains_key(&found.id) && portal.users.len() >= USERS_MAX {
        drop(state);
        let full = pick(de, "Das Portal ist voll. Frag die Orga-Leitung.", "The portal is full. Ask the org's leadership.");
        return message(server, de, 503, Tone::Warn, full).with_header("Set-Cookie", &clear_login);
    }
    let user = portal.users.entry(found.id.clone()).or_insert_with(|| PortalUser { first: now, ..PortalUser::default() });
    user.name = found.name.clone();
    user.username = found.username;
    user.roles = found.roles;
    user.last = now;
    let manual = user.manual;
    let role = effective(user, &portal.map);
    let Some(role) = role else {
        server.save(&state);
        drop(state);
        let text = if manual == Manual::Blocked {
            pick(de, "Dein Zugang zum Portal ist gesperrt. Frag die Orga-Leitung.", "Your access to the portal is blocked. Ask the org's leadership.")
                .to_string()
        } else {
            pick(
                de,
                "Hallo {n}. Du bist angemeldet, aber noch nicht freigeschaltet. Die Orga-Leitung sieht deine Anfrage und schaltet dich frei.",
                "Hello {n}. You are logged in but not let in yet. The org's leadership sees your request and lets you in.",
            )
            .replace("{n}", &found.name)
        };
        return message(server, de, 403, Tone::Warn, &text).with_header("Set-Cookie", &clear_login);
    };
    let token = server.random_hex(32);
    let csrf = server.random_hex(16);
    let portal = &mut state.portal;
    portal.sessions.retain(|_, s| s.created + SESSION_SECONDS > now && s.seen + IDLE_SECONDS > now);
    let mut own: Vec<(String, u64)> =
        portal.sessions.iter().filter(|(_, s)| s.user == found.id).map(|(k, s)| (k.clone(), s.created)).collect();
    own.sort_by_key(|(_, created)| *created);
    for (key, _) in own.iter().take((own.len() + 1).saturating_sub(SESSIONS_PER_USER)) {
        portal.sessions.remove(key);
    }
    while portal.sessions.len() >= SESSIONS_MAX {
        let Some(oldest) = portal.sessions.iter().min_by_key(|(_, s)| s.created).map(|(k, _)| k.clone()) else { break };
        portal.sessions.remove(&oldest);
    }
    portal.sessions.insert(hash(&token), PortalSession { user: found.id.clone(), csrf, created: now, seen: now });
    server.save(&state);
    drop(state);
    let config = server.config();
    let body = format!(
        "<h1>{title}</h1>{note}<div class=\"knopfreihe\"><a class=\"knopf voll\" href=\"{ROOT}\">{go}{next}</a></div>",
        title = pick(de, "Mitglieder-Portal", "Members' portal"),
        note = note_box(
            Tone::Good,
            &pick(de, "Angemeldet als {n} ({r}).", "Logged in as {n} ({r}).").replace("{n}", &found.name).replace("{r}", role_label(role, de)),
        ),
        go = icon("arrow-left"),
        next = pick(de, "Weiter zum Portal", "On to the portal"),
    );
    // A page of this server leads on instead of a redirect: a cookie with SameSite=Strict is sent from the next
    // page on, not on a redirect that started at Discord.
    let head = format!("<meta http-equiv=\"refresh\" content=\"0;url={ROOT}\">");
    let page = page_with_head(de, &config.name, &config.tag, pick(de, "Mitglieder-Portal", "Members' portal"), &body, &head);
    admin::language(Response::html(200, page), de)
        .with_header("Set-Cookie", &format!("{}={token}; Max-Age={SESSION_SECONDS}{}", session_cookie(server), cookie_attributes(server, true)))
        .with_header("Set-Cookie", &clear_login)
}

/// The people of the portal for the lead: role, where it comes from, last login, and the role by hand.
pub(crate) fn access_page(ctx: &Ctx) -> Response {
    let q = ctx.query("q");
    let needle = q.to_lowercase();
    let wanted = match ctx.query("status").as_str() {
        "waiting" => "waiting",
        "blocked" => "blocked",
        _ => "all",
    };
    let me = match &ctx.actor {
        Actor::Member { id, .. } => id.clone(),
        Actor::Admin => String::new(),
    };
    let state = ctx.server.lock();
    let portal = &state.portal;
    let total = portal.users.len();
    let mut people: Vec<(&String, &PortalUser, Option<Role>)> =
        portal.users.iter().map(|(id, u)| (id, u, effective(u, &portal.map))).collect();
    people.sort_by_key(|(_, u, role)| (role.is_some(), u.manual == Manual::Blocked, u.name.to_lowercase()));
    let rows: Vec<String> = people
        .iter()
        .filter(|(_, u, role)| match wanted {
            "waiting" => role.is_none() && u.manual != Manual::Blocked,
            "blocked" => u.manual == Manual::Blocked,
            _ => true,
        })
        .filter(|(id, u, _)| found(&needle, &[&u.name, &u.username, id]))
        .map(|(id, u, role)| {
            let mark = match (role, u.manual) {
                (_, Manual::Blocked) => badge(ctx.say("Gesperrt", "Blocked"), "rot"),
                (Some(role), _) => badge(role_label(*role, ctx.de), "an"),
                (None, _) => badge(ctx.say("Wartet", "Waiting"), "aus"),
            };
            let source = if u.manual == Manual::Auto { ctx.say("aus den Discord-Rollen", "from the Discord roles") } else { ctx.say("von Hand", "by hand") };
            let roles = if u.roles.is_empty() {
                "–".to_string()
            } else {
                u.roles.iter().map(|r| format!("<code>{}</code>", escape_html(r))).collect::<Vec<_>>().join(" ")
            };
            let form = if **id == me {
                format!("<p class=\"klein\">{}</p>", ctx.say("Das bist du. Deine eigene Rolle ändert eine andere Person der Leitung.", "This is you. Another person of the lead changes your own role."))
            } else {
                let options: String = Manual::ALL
                    .iter()
                    .map(|m| format!("<option value=\"{}\"{}>{}</option>", m.code(), if *m == u.manual { " selected" } else { "" }, m.label(ctx.de)))
                    .collect();
                format!(
                    "<form class=\"suche\" method=\"post\" action=\"{set}\">{hidden}<input type=\"hidden\" name=\"user\" value=\"{id}\">\
                     <label class=\"unsichtbar\" for=\"r{id}\">{role_word}</label><select id=\"r{id}\" name=\"manual\">{options}</select>\
                     <button class=\"knopf klein\" type=\"submit\" aria-label=\"{aria}\">{save}{save_label}</button></form>\
                     <form method=\"post\" action=\"{forget}\">{hidden}<input type=\"hidden\" name=\"user\" value=\"{id}\">\
                     <button class=\"knopf klein\" type=\"submit\" aria-label=\"{forget_aria}\">{x}{forget_label}</button></form>",
                    set = ctx.at("/access/role"),
                    forget = ctx.at("/access/forget"),
                    hidden = ctx.hidden(),
                    id = escape_html(id),
                    role_word = ctx.say("Rolle", "Role"),
                    aria = escape_html(&ctx.say("Rolle von {n} speichern", "Save the role of {n}").replace("{n}", &u.name)),
                    save = icon("save"),
                    save_label = ctx.say("Speichern", "Save"),
                    forget_aria = escape_html(&ctx.say("{n} vergessen", "Forget {n}").replace("{n}", &u.name)),
                    x = icon("circle-x"),
                    forget_label = ctx.say("Vergessen", "Forget"),
                )
            };
            format!(
                "<li><div class=\"zeile-kopf\"><span class=\"titel\">{name}</span>{mark}</div>\
                 <dl class=\"paare\"><dt>Discord</dt><dd>@{username} · <code>{id}</code></dd>\
                 <dt>{source_label}</dt><dd>{source}</dd><dt>{last_label}</dt><dd>{last}</dd>\
                 <dt>{roles_label}</dt><dd>{roles}</dd></dl>{form}</li>",
                name = escape_html(&u.name),
                username = escape_html(&u.username),
                id = escape_html(id),
                source_label = ctx.say("Rolle", "Role"),
                last_label = ctx.say("Letzte Anmeldung", "Last login"),
                last = moment(u.last),
                roles_label = ctx.say("Discord-Rollen", "Discord roles"),
            )
        })
        .collect();
    let unmapped = portal.map.is_empty();
    drop(state);
    let note = if !ctx.query("gespeichert").is_empty() {
        note_box(Tone::Good, ctx.say("Gespeichert.", "Saved."))
    } else if !ctx.query("vergessen").is_empty() {
        note_box(Tone::Good, ctx.say("Vergessen. Meldet sich die Person wieder an, steht sie wieder hier.", "Forgotten. If the person logs in again, they show here again."))
    } else if unmapped {
        note_box(
            Tone::Warn,
            ctx.say(
                "Noch keine Discord-Rolle zugeordnet: Jede Anmeldung wartet hier, bis du eine Rolle von Hand vergibst. Zuordnen unter Einstellungen.",
                "No Discord role mapped yet: every login waits here until you give a role by hand. Map them under Settings.",
            ),
        )
    } else {
        String::new()
    };
    let options: String = [("all", ctx.say("Alle", "All")), ("waiting", ctx.say("Wartet", "Waiting")), ("blocked", ctx.say("Gesperrt", "Blocked"))]
        .iter()
        .map(|(v, l)| format!("<option value=\"{v}\"{}>{l}</option>", if *v == wanted { " selected" } else { "" }))
        .collect();
    let select = format!("<label class=\"unsichtbar\" for=\"s\">{}</label><select id=\"s\" name=\"status\">{options}</select>", ctx.say("Zustand", "State"));
    let body = format!(
        "{note}<p class=\"einleitung\">{intro}</p>{search}{count}{card}<p class=\"hinweis\">{hint}</p>",
        intro = ctx.say(
            "Wer sich mit Discord angemeldet hat. Die Rolle kommt aus den Discord-Rollen oder von Hand; „Gesperrt“ hält jemanden draußen, was Discord auch sagt.",
            "Who logged in with Discord. The role comes from the Discord roles or by hand; \"Blocked\" keeps someone out whatever Discord says.",
        ),
        search = search_form(ctx, &ctx.at("/access"), &q, &select, !q.is_empty() || wanted != "all", ctx.say("Name oder Discord-ID", "Name or Discord id")),
        count = count_line(ctx, rows.len(), total),
        card = list_card("user-check", ctx.say("Zugang zum Portal", "Access to the portal"), &rows, ctx.say("Noch niemand.", "Nobody yet.")),
        hint = ctx.say(
            "Rollen aus Discord gelten ab der nächsten Anmeldung; eine Sitzung dauert höchstens 8 Stunden. Sperren und Rollen von Hand gelten sofort. „Vergessen“ löscht den Eintrag; wer gesperrt bleiben soll, bleibt besser gesperrt.",
            "Roles from Discord count from the next login; a session lasts at most 8 hours. Blocking and roles by hand count at once. \"Forget\" deletes the entry; whoever should stay out had better stay blocked.",
        ),
    );
    shell(ctx, "/access", ctx.say("Zugang", "Access"), &body, 200)
}

/// The person a form of the access page names, unless it is the actor.
fn target(ctx: &Ctx) -> Result<String, Response> {
    let form = ctx.request.form();
    let id = value(&form, "user").unwrap_or_default().to_string();
    if matches!(&ctx.actor, Actor::Member { id: me, .. } if *me == id) {
        let own = ctx.say("Deine eigene Rolle ändert eine andere Person der Leitung.", "Another person of the lead changes your own role.");
        return Err(shell(ctx, "/access", ctx.say("Zugang", "Access"), &note_box(Tone::Warn, own), 409));
    }
    if !ctx.server.lock().portal.users.contains_key(&id) {
        return Err(shell(ctx, "/access", ctx.say("Zugang", "Access"), &admin::missing(ctx), 404));
    }
    Ok(id)
}

/// Sets the role by hand; every request reads the role again, so a person without a role afterwards is out at the
/// next request.
pub(crate) fn set_role(ctx: &Ctx) -> Response {
    let id = match target(ctx) {
        Ok(id) => id,
        Err(answer) => return answer,
    };
    let form = ctx.request.form();
    let Some(manual) = value(&form, "manual").and_then(Manual::from_code) else {
        let wrong = ctx.say("Diese Rolle gibt es nicht.", "There is no such role.");
        return shell(ctx, "/access", ctx.say("Zugang", "Access"), &note_box(Tone::Warn, wrong), 422);
    };
    let mut state = ctx.server.lock();
    if let Some(user) = state.portal.users.get_mut(&id) {
        user.manual = manual;
    }
    ctx.server.save(&state);
    drop(state);
    redirect(&format!("{}?gespeichert=1", ctx.at("/access")))
}

/// Deletes a person and every session of it.
pub(crate) fn forget(ctx: &Ctx) -> Response {
    let id = match target(ctx) {
        Ok(id) => id,
        Err(answer) => return answer,
    };
    let mut state = ctx.server.lock();
    state.portal.users.remove(&id);
    state.portal.sessions.retain(|_, s| s.user != id);
    ctx.server.save(&state);
    drop(state);
    redirect(&format!("{}?vergessen=1", ctx.at("/access")))
}

/// The Discord ids in a field, separated by spaces, commas or semicolons; the error names the field.
fn ids(text: &str, field: &str, de: bool) -> Result<Vec<String>, String> {
    let mut list: Vec<String> = Vec::new();
    for part in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|p| !p.is_empty()) {
        if !snowflake(part) {
            return Err(pick(de, "{f}: „{v}“ ist keine Rollen-ID (17 bis 20 Ziffern).", "{f}: \"{v}\" is no role id (17 to 20 digits).")
                .replace("{f}", field)
                .replace("{v}", &part.chars().take(40).collect::<String>()));
        }
        if !list.iter().any(|l| l == part) {
            list.push(part.to_string());
        }
    }
    if list.len() > ROLES_MAX {
        return Err(pick(de, "{f}: höchstens 50 Rollen.", "{f}: at most 50 roles.").replace("{f}", field));
    }
    Ok(list)
}

/// The settings of the portal: the state of the Discord connection, the address to register, and the role map.
/// `typed` holds a form that did not pass, with its error.
pub(crate) fn settings_page(ctx: &Ctx, typed: Option<(&RoleMap, [&str; 3], &str)>, status: u16) -> Response {
    let config = ctx.server.config();
    let stored = ctx.server.lock().portal.map.clone();
    let (map, fields, error) = match typed {
        Some((map, fields, error)) => (map.clone(), fields.map(str::to_string), Some(error)),
        None => (stored.clone(), [stored.member.join(" "), stored.logistics.join(" "), stored.lead.join(" ")], None),
    };
    let connection = match &config.discord {
        Some(discord) => format!(
            "<dl class=\"werte\"><dt>{state}</dt><dd>{on}</dd><dt>{guild}</dt><dd><code>{guild_id}</code></dd>\
             <dt>{client}</dt><dd><code>{client_id}</code></dd><dt>{uri}</dt><dd><code>{redirect}</code></dd></dl>\
             <p class=\"hinweis\">{uri_hint}</p>",
            state = ctx.say("Zustand", "State"),
            on = badge(ctx.say("An", "On"), "an"),
            guild = ctx.say("Discord-Server-ID", "Discord server id"),
            guild_id = escape_html(&discord.guild_id),
            client = ctx.say("Client-ID der App", "Client id of the app"),
            client_id = escape_html(&discord.client_id),
            uri = ctx.say("Weiterleitungs-Adresse", "Redirect address"),
            redirect = escape_html(&redirect_uri(ctx.server)),
            uri_hint = ctx.say(
                "Genau diese Adresse steht in der Discord-App unter OAuth2 › Redirects.",
                "Exactly this address is listed in the Discord app under OAuth2 › Redirects.",
            ),
        ),
        None => format!(
            "<dl class=\"werte\"><dt>{state}</dt><dd>{off}</dd></dl><p>{how}</p>",
            state = ctx.say("Zustand", "State"),
            off = badge(ctx.say("Aus", "Off"), "aus"),
            how = ctx.say(
                "Ohne Discord-App ist das Portal aus. Setzt VERSEKIT_ORG_DISCORD_CLIENT_ID, VERSEKIT_ORG_DISCORD_CLIENT_SECRET und VERSEKIT_ORG_DISCORD_GUILD_ID in der Datei .env und startet den Server neu. Die Rollen unten könnt ihr schon jetzt zuordnen.",
                "Without a Discord app the portal is off. Set VERSEKIT_ORG_DISCORD_CLIENT_ID, VERSEKIT_ORG_DISCORD_CLIENT_SECRET and VERSEKIT_ORG_DISCORD_GUILD_ID in the file .env and restart the server. You can map the roles below already.",
            ),
        ),
    };
    let note = match error {
        Some(error) => note_box(Tone::Warn, error),
        None if !ctx.query("gespeichert").is_empty() => note_box(Tone::Good, ctx.say("Gespeichert. Die Rollen gelten sofort.", "Saved. The roles count at once.")),
        None => String::new(),
    };
    let field = |id: &str, name: &str, label: &str, value: &str| {
        format!(
            "<label class=\"feld\" for=\"{id}\">{label}</label>\
             <input id=\"{id}\" type=\"text\" name=\"{name}\" value=\"{value}\" inputmode=\"numeric\" autocomplete=\"off\" spellcheck=\"false\">",
            value = escape_html(value),
        )
    };
    let body = format!(
        "{note}<section class=\"karte\"><h2>{plug}{discord_head}</h2>{connection}</section>\
         <section class=\"karte\"><h2>{users}{map_head}</h2><p>{map_intro}</p>\
         <form method=\"post\" action=\"{action}\">{hidden}{lead}{logistics}{member}\
         <label class=\"recht\"><input type=\"checkbox\" name=\"everyone\" value=\"1\"{checked}><span>{everyone}</span></label>\
         <p class=\"klein\">{everyone_hint}</p>\
         <div class=\"knopfreihe\"><button class=\"knopf voll\" type=\"submit\">{save}{save_label}</button></div></form>\
         <p class=\"hinweis\">{ids_hint}</p></section>",
        plug = icon("settings"),
        discord_head = ctx.say("Discord-Anmeldung", "Discord login"),
        users = icon("users"),
        map_head = ctx.say("Rollen aus Discord", "Roles from Discord"),
        map_intro = ctx.say(
            "Trag die IDs der Discord-Rollen ein, mehrere mit Leerzeichen getrennt. Wer mehrere hat, bekommt die höchste.",
            "Enter the ids of the Discord roles, several separated by spaces. Whoever holds several gets the highest.",
        ),
        action = ctx.at("/settings"),
        hidden = ctx.hidden(),
        lead = field("rl", "lead", ctx.say("Leitung: alles, dazu Mitglieder, Zugang und Einstellungen", "Lead: everything, also members, access and settings"), &fields[2]),
        logistics = field("rg", "logistics", ctx.say("Logistik: dazu Aufträge anlegen und Lieferungen bestätigen", "Logistics: also create orders and confirm deliveries"), &fields[1]),
        member = field("rm", "member", ctx.say("Mitglied: Bestand, Baupläne, Aufträge und Lieferungen ansehen", "Member: see stock, blueprints, orders and deliveries"), &fields[0]),
        checked = if map.everyone { " checked" } else { "" },
        everyone = ctx.say("Jedes Mitglied des Discord-Servers ist Mitglied im Portal", "Every member of the Discord server is a member in the portal"),
        everyone_hint = ctx.say(
            "Aus ab Werk. Nur einschalten, wenn auf dem Discord-Server keine Gäste sind: Sonst sehen auch sie Bestand und Baupläne.",
            "Off by default. Only turn it on if there are no guests on the Discord server: else they see stock and blueprints too.",
        ),
        save = icon("save"),
        save_label = ctx.say("Speichern", "Save"),
        ids_hint = ctx.say(
            "Rollen-ID finden: In Discord unter Einstellungen › Erweitert den Entwicklermodus einschalten, dann Servereinstellungen › Rollen, Rechtsklick auf die Rolle › Rollen-ID kopieren.",
            "Finding a role id: in Discord turn on the developer mode under Settings › Advanced, then Server settings › Roles, right-click the role › Copy Role ID.",
        ),
    );
    shell(ctx, "/settings", ctx.say("Einstellungen", "Settings"), &body, status)
}

/// Saves the role map after checking every id; the people keep their roles by hand.
pub(crate) fn save_settings(ctx: &Ctx) -> Response {
    let form = ctx.request.form();
    let field = |name: &str| value(&form, name).unwrap_or_default().to_string();
    let (member, logistics, lead) = (field("member"), field("logistics"), field("lead"));
    let everyone = value(&form, "everyone") == Some("1");
    let checked = (|| -> Result<RoleMap, String> {
        Ok(RoleMap {
            lead: ids(&lead, ctx.say("Leitung", "Lead"), ctx.de)?,
            logistics: ids(&logistics, ctx.say("Logistik", "Logistics"), ctx.de)?,
            member: ids(&member, ctx.say("Mitglied", "Member"), ctx.de)?,
            everyone,
        })
    })();
    match checked {
        Ok(map) => {
            let mut state = ctx.server.lock();
            state.portal.map = map;
            ctx.server.save(&state);
            drop(state);
            redirect(&format!("{}?gespeichert=1", ctx.at("/settings")))
        }
        Err(error) => {
            let typed = RoleMap { everyone, ..RoleMap::default() };
            settings_page(ctx, Some((&typed, [&member, &logistics, &lead], &error)), 422)
        }
    }
}

/// Whether a request belongs to the portal.
pub(crate) fn owns(path: &str) -> bool {
    path == ROOT || path.starts_with("/portal/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_and_the_pkce_challenge_follow_the_rfcs() {
        assert_eq!(base64(b"", false), "");
        assert_eq!(base64(b"f", false), "Zg==");
        assert_eq!(base64(b"fo", false), "Zm8=");
        assert_eq!(base64(b"foobar", false), "Zm9vYmFy");
        assert_eq!(base64(&[0xfb, 0xff], true), "-_8");
        // The value Python's hashlib.sha256 and base64.urlsafe_b64encode give for the same verifier.
        assert_eq!(challenge("dBjftJeZ4CVP-mJ92K1PdHtvoqwjj7O7r3oNnUgnu9M"), "0QxjMJrz8qVBDQ_6nWn6UvSyRSoSYi4CnKbr7MSLaNc");
    }

    #[test]
    fn ids_names_and_roles_are_checked() {
        assert!(snowflake("81384788765712384") && snowflake("12345678901234567890"));
        assert!(!snowflake("1234") && !snowflake("8138478876571238x") && !snowflake("123456789012345678901"));
        assert_eq!(clean_name(" Ve\u{7}ga \n"), "Vega");
        assert_eq!(clean_name("\u{202E}agev\u{200B}\u{2066}x\u{FEFF}"), "agevx", "nothing turns or hides the name");
        assert!(ids("81384788765712384, 81384788765712385;81384788765712384", "x", true).unwrap().len() == 2);
        assert!(ids("Leitung", "x", true).is_err());
        assert!(Discord::new("81384788765712384", "kurz", "81384788765712384").is_err());
        let discord = Discord::new("81384788765712384", "0123456789abcdef0123", "81384788765712385").unwrap();
        assert!(!format!("{discord:?}").contains("0123456789abcdef"), "the secret never shows");
        let map = RoleMap { member: vec!["1".into()], logistics: vec!["2".into()], lead: vec!["3".into()], everyone: false };
        let user = |roles: &[&str], manual| PortalUser { roles: roles.iter().map(|r| r.to_string()).collect(), manual, ..PortalUser::default() };
        assert_eq!(effective(&user(&["1", "3"], Manual::Auto), &map), Some(Role::Lead), "the highest role counts");
        assert_eq!(effective(&user(&["2"], Manual::Auto), &map), Some(Role::Logistics));
        assert_eq!(effective(&user(&[], Manual::Auto), &map), None, "without a role the login waits");
        assert_eq!(effective(&user(&["3"], Manual::Blocked), &map), None, "blocked beats Discord");
        assert_eq!(effective(&user(&[], Manual::Member), &map), Some(Role::Member));
        let all = RoleMap { everyone: true, ..RoleMap::default() };
        assert_eq!(effective(&user(&[], Manual::Auto), &all), Some(Role::Member));
    }
}
