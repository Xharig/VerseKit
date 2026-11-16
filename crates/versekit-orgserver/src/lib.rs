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

//! A template server of the open org standard `versekit-org` v1, for an org
//! to host itself (`tools/orga-standard/v1/openapi.yaml`).
//!
//! | Part | Does |
//! |---|---|
//! | discovery | `/.well-known/versekit-org.json` |
//! | device login | `device/code`, `token` (device code and refresh token), `revoke` |
//! | panel | `/device`: a member types the code and a personal key, sees the rights and confirms |
//! | status | `/`: a read-only page with name, version, running since, number of connected members, rights offered |
//! | API | `me`, `stock`, `stock/mine`, `blueprints`, `blueprints/mine`, `orders`, `orders/{id}/deliveries` |
//! | administration | `/admin`: a layer of this server only, never part of the standard (`admin.rs`) |
//! | portal | `/portal`: members log in with Discord and see the same pages as far as their role allows (`portal.rs`) |
//! | health | `/healthz` for the health check of the container |
//!
//! Tokens, device codes and member keys are kept as SHA-256 hashes only, the admin password as PBKDF2.
//! Pending logins and admin sessions live in memory; tokens, the shared data and the orders go into the state file
//! after every change. Unless in the explicit test mode, every request must come over HTTPS through the reverse
//! proxy in front, and every answer carries the security headers.

mod admin;
pub mod conformance;
pub mod http;
mod look;
pub mod net;
mod panel;
pub mod portal;
pub mod setup;
mod status;
pub mod updates;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{Map, Value, json};

use crate::http::{Request, Response, value};

pub const STANDARD: &str = "versekit-org";
pub const VERSION: &str = "1";
pub const WELL_KNOWN: &str = "/.well-known/versekit-org.json";
pub const API_PATH: &str = "/versekit/v1";
pub const PANEL_PATH: &str = "/device";
pub const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// The rights of the standard, version 1.
pub const SCOPES: [&str; 6] =
    ["stock:read", "stock:write", "blueprints:read", "blueprints:write", "orders:read", "orders:write"];
/// Lifetime of an access token.
pub const ACCESS_SECONDS: u64 = 3600;
/// Lifetime of a device code and its user code.
pub const CODE_SECONDS: u64 = 600;
/// Requests per minute and connection before `429`, unless the configuration names another number.
pub const RATE_LIMIT: u32 = 120;
/// Entries of a list page without `limit`, and the most with it.
pub const PAGE_DEFAULT: usize = 500;
pub const PAGE_MAX: usize = 1000;
/// Wrong member keys for one user code before it is dropped.
pub const PANEL_TRIES: u32 = 5;
/// The format of the state file this version writes; older ones are moved after a backup, newer ones refused.
pub const STATE_FORMAT: u64 = 3;
const CODE_LETTERS: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";
const MAX_TEXT: usize = 200;
const MAX_ITEMS: usize = 5000;
const MAX_BLUEPRINTS: usize = 10_000;
const MIN_KEY: usize = 16;
/// A refresh token not used for so long ends the login.
pub const REFRESH_IDLE: u64 = 90 * 86_400;
/// Logins waiting for a confirmation at the same time; more are refused with 429.
pub const PENDING_MAX: usize = 500;
/// Wrong member keys on the panel per minute over all codes before it refuses for the rest of the minute.
pub const PANEL_WRONG_PER_MINUTE: u32 = 30;
/// The largest quantity of a delivered or ordered good.
pub const QUANTITY_MAX: f64 = 1e9;
/// The deliveries kept in the log of the administration; older ones are dropped.
const DELIVERY_LOG_MAX: usize = 10_000;
/// The keys an older config.example.json carried; they are public and never count as a member's key.
const EXAMPLE_KEYS: [&str; 2] = ["mindestens-16-zeichen-zufall-1", "mindestens-16-zeichen-zufall-2"];

/// One member of the org with the hash of the personal key for the panel.
#[derive(Clone, Debug)]
pub struct Member {
    pub id: String,
    pub name: String,
    key_hash: String,
}

/// What the org sets up.
#[derive(Clone, Debug)]
pub struct Config {
    /// The public address, `https://host[:port]` without a path.
    pub public_url: String,
    pub name: String,
    pub tag: String,
    pub homepage: Option<String>,
    pub members: Vec<Member>,
    /// The orders a fresh state starts with.
    pub orders: Vec<Value>,
    /// Seconds between two polls of the token endpoint.
    pub poll_interval: u64,
    /// Requests per minute and connection before `429`.
    pub rate_limit: u32,
    /// Where the state is kept; `None` keeps it in memory only.
    pub state_file: Option<PathBuf>,
    /// The admin password as a hash; `None` leaves `/admin` off.
    admin: Option<admin::AdminKey>,
    /// Where the admin password comes from, for the self-check.
    pub admin_source: setup::AdminSource,
    /// The explicit test mode (`VERSEKIT_ORG_TESTBETRIEB=1`): plain HTTP on the own computer, a note on the status page.
    pub test_mode: bool,
    /// Every request must come over HTTPS through the reverse proxy; every answer carries HSTS.
    pub require_https: bool,
    /// Ask GitHub once a day for a newer server.
    pub update_check: bool,
    /// The Discord application of the org; `None` leaves the portal off.
    pub discord: Option<portal::Discord>,
    /// The rights of the standard this server offers (`scopes_supported`): all of them unless the org offers fewer.
    pub scopes_offered: Vec<&'static str>,
}

fn loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

/// `https://host[:port]`, or `http://` on the own computer; no path, no user.
pub fn origin_ok(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once("://") else { return false };
    if rest.is_empty() || rest.contains(['/', '?', '#', '@', '\\', ' ']) {
        return false;
    }
    let (host, port) = match rest.strip_prefix('[') {
        Some(after) => {
            let Some((inner, tail)) = after.split_once(']') else { return false };
            if !tail.is_empty() && !tail.starts_with(':') {
                return false;
            }
            (format!("[{inner}]"), tail.strip_prefix(':'))
        }
        None => match rest.split_once(':') {
            Some((host, port)) => (host.to_string(), Some(port)),
            None => (rest.to_string(), None),
        },
    };
    if host.is_empty() || port.is_some_and(|p| p.parse::<u16>().is_err()) {
        return false;
    }
    match scheme {
        "https" => true,
        "http" => loopback_host(&host),
        _ => false,
    }
}

fn text_of<'a>(object: &'a Value, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

impl Config {
    /// A configuration with no members and no orders.
    pub fn new(public_url: &str, name: &str, tag: &str) -> Config {
        Config {
            public_url: public_url.trim_end_matches('/').to_string(),
            name: name.to_string(),
            tag: tag.to_string(),
            homepage: None,
            members: Vec::new(),
            orders: Vec::new(),
            poll_interval: 5,
            rate_limit: RATE_LIMIT,
            state_file: None,
            admin: None,
            admin_source: setup::AdminSource::None,
            test_mode: false,
            require_https: false,
            update_check: false,
            discord: None,
            scopes_offered: SCOPES.to_vec(),
        }
    }

    /// The same configuration offering only these rights of the standard; the others are not named and are refused.
    pub fn with_scopes_offered(mut self, offered: &[&str]) -> Config {
        self.scopes_offered = SCOPES.iter().copied().filter(|s| offered.contains(s)).collect();
        self
    }

    /// The same configuration with the Discord application of the org, which turns the portal on.
    pub fn with_discord(mut self, client_id: &str, secret: &str, guild_id: &str) -> Result<Config, String> {
        self.discord = Some(portal::Discord::new(client_id, secret, guild_id)?);
        Ok(self)
    }

    /// The same configuration with this admin password (at least 12 characters), as set through the environment.
    pub fn with_admin_password(mut self, password: &str) -> Result<Config, String> {
        self.admin = Some(admin::AdminKey::new(password)?);
        self.admin_source = setup::AdminSource::Environment;
        Ok(self)
    }

    /// The same configuration with the admin password the server generated into `file`.
    pub fn with_generated_password(mut self, password: &str, file: PathBuf) -> Result<Config, String> {
        self.admin = Some(admin::AdminKey::new(password)?);
        self.admin_source = setup::AdminSource::File(file);
        Ok(self)
    }

    /// Whether `/admin` has a password.
    pub fn admin_enabled(&self) -> bool {
        self.admin.is_some()
    }

    /// The same configuration with one member more.
    pub fn with_member(mut self, id: &str, name: &str, key: &str) -> Config {
        self.members.push(Member { id: id.to_string(), name: name.to_string(), key_hash: hash(key) });
        self
    }

    /// The same configuration with one order more (`{"id", "title", "items": [{"item", "quantity", "unit"}]}`).
    pub fn with_order(mut self, order: Value) -> Config {
        self.orders.push(order);
        self
    }

    /// Reads the configuration file (JSON); the error says what is wrong.
    pub fn from_json(text: &str) -> Result<Config, String> {
        let root: Value = serde_json::from_str(text).map_err(|e| format!("config is not valid JSON: {e}"))?;
        let public_url = text_of(&root, "public_url").ok_or("public_url is missing")?;
        let name = text_of(&root, "name").ok_or("name is missing")?;
        let tag = text_of(&root, "tag").unwrap_or_default();
        let mut config = Config::new(public_url, name, tag);
        config.homepage = text_of(&root, "homepage").map(str::to_string);
        if let Some(interval) = root.get("poll_interval") {
            config.poll_interval = interval.as_u64().ok_or("poll_interval is no whole number")?;
        }
        if let Some(limit) = root.get("rate_limit") {
            config.rate_limit = limit
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or("rate_limit is no whole number above 0")?;
        }
        for member in root.get("members").and_then(Value::as_array).into_iter().flatten() {
            let id = text_of(member, "id").ok_or("a member has no id")?;
            let name = text_of(member, "name").unwrap_or(id);
            let key = member.get("key").and_then(Value::as_str).unwrap_or_default();
            if key.chars().count() < MIN_KEY {
                return Err(format!(
                    "the key of member {id} is shorter than {MIN_KEY} characters; give every member an own random key \
                     (openssl rand -base64 18)"
                ));
            }
            if EXAMPLE_KEYS.contains(&key) {
                return Err(format!("member {id} still has the example key of an older template; give every member an own random key"));
            }
            config = config.with_member(id, name, key);
        }
        for order in root.get("orders").and_then(Value::as_array).into_iter().flatten() {
            config.orders.push(order.clone());
        }
        config.check()?;
        Ok(config)
    }

    /// The same configuration with what the environment sets: `VERSEKIT_ORG_PUBLIC_URL`, `VERSEKIT_ORG_NAME`,
    /// `VERSEKIT_ORG_TAG`, `VERSEKIT_ORG_RATE_LIMIT`, `VERSEKIT_ORG_POLL_INTERVAL`, `VERSEKIT_ORG_TESTBETRIEB` (also
    /// `TESTBETRIEB`), `VERSEKIT_ORG_UPDATE_CHECK`, `VERSEKIT_ORG_ADMIN_PASSWORD` or `VERSEKIT_ORG_ADMIN_PASSWORD_FILE`,
    /// `VERSEKIT_ORG_ADMIN`; `get` reads one variable, an empty one counts as not set. This is the way the program
    /// starts: outside the test mode the address must be https and every request must come over HTTPS. The error says
    /// which value is wrong, never the password.
    pub fn with_env(mut self, get: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let raw = |name: &str| get(name).filter(|v| !v.trim().is_empty());
        let get = |name: &str| get(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let on = |value: Option<String>| value.is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "ja" | "yes" | "on"));
        self.test_mode = on(get("VERSEKIT_ORG_TESTBETRIEB")) || on(get("TESTBETRIEB"));
        self.require_https = !self.test_mode;
        self.update_check = get("VERSEKIT_ORG_UPDATE_CHECK").is_none_or(|v| v != "0");
        let off = get("VERSEKIT_ORG_ADMIN").is_some_and(|v| v.eq_ignore_ascii_case("off"));
        if off {
            self.admin = None;
            self.admin_source = setup::AdminSource::Off;
        } else if let Some(password) = raw("VERSEKIT_ORG_ADMIN_PASSWORD") {
            self = self.with_admin_password(&password).map_err(|e| format!("VERSEKIT_ORG_ADMIN_PASSWORD: {e}"))?;
        } else if let Some(file) = get("VERSEKIT_ORG_ADMIN_PASSWORD_FILE") {
            let text = std::fs::read_to_string(&file).map_err(|e| format!("VERSEKIT_ORG_ADMIN_PASSWORD_FILE cannot be read: {e}"))?;
            self = self
                .with_admin_password(text.trim_end_matches(['\r', '\n']))
                .map_err(|e| format!("VERSEKIT_ORG_ADMIN_PASSWORD_FILE: {e}"))?;
        }
        if let Some(url) = get("VERSEKIT_ORG_PUBLIC_URL") {
            self.public_url = url.trim_end_matches('/').to_string();
        }
        if let Some(name) = get("VERSEKIT_ORG_NAME") {
            self.name = name;
        }
        if let Some(tag) = get("VERSEKIT_ORG_TAG") {
            self.tag = tag;
        }
        if let Some(limit) = get("VERSEKIT_ORG_RATE_LIMIT") {
            self.rate_limit =
                limit.parse().ok().filter(|n| *n > 0).ok_or("VERSEKIT_ORG_RATE_LIMIT is no whole number above 0")?;
        }
        if let Some(interval) = get("VERSEKIT_ORG_POLL_INTERVAL") {
            self.poll_interval = interval.parse().map_err(|_| "VERSEKIT_ORG_POLL_INTERVAL is no whole number")?;
        }
        let client_id = get("VERSEKIT_ORG_DISCORD_CLIENT_ID");
        let guild_id = get("VERSEKIT_ORG_DISCORD_GUILD_ID");
        let secret = match (get("VERSEKIT_ORG_DISCORD_CLIENT_SECRET"), get("VERSEKIT_ORG_DISCORD_CLIENT_SECRET_FILE")) {
            (Some(secret), _) => Some(secret),
            (None, Some(file)) => Some(
                std::fs::read_to_string(&file)
                    .map_err(|e| format!("VERSEKIT_ORG_DISCORD_CLIENT_SECRET_FILE cannot be read: {e}"))?
                    .trim()
                    .to_string(),
            ),
            (None, None) => None,
        };
        match (client_id, secret, guild_id) {
            (None, None, None) => self.discord = None,
            (Some(client_id), Some(secret), Some(guild_id)) => {
                self.discord = Some(portal::Discord::new(&client_id, &secret, &guild_id)?);
            }
            (client_id, secret, guild_id) => {
                let missing: Vec<&str> = [
                    (client_id.is_none(), "VERSEKIT_ORG_DISCORD_CLIENT_ID"),
                    (secret.is_none(), "VERSEKIT_ORG_DISCORD_CLIENT_SECRET"),
                    (guild_id.is_none(), "VERSEKIT_ORG_DISCORD_GUILD_ID"),
                ]
                .iter()
                .filter(|(gone, _)| *gone)
                .map(|(_, name)| *name)
                .collect();
                return Err(format!(
                    "the portal needs all three Discord settings, missing: {}; set them in .env, or remove the others to leave the portal off",
                    missing.join(", ")
                ));
            }
        }
        self.check()?;
        if self.public_url.starts_with("http://") && !self.test_mode {
            return Err("public_url is http: only in the test mode (VERSEKIT_ORG_TESTBETRIEB=1), never for real members; \
                        use https://your-domain with a reverse proxy"
                .into());
        }
        Ok(self)
    }

    /// Whether the configuration can serve.
    pub fn check(&self) -> Result<(), String> {
        if !origin_ok(&self.public_url) {
            return Err("public_url must be https://host[:port] without a path (http only on 127.0.0.1)".into());
        }
        let mut ids: Vec<&str> = self.members.iter().map(|m| m.id.as_str()).collect();
        ids.sort_unstable();
        if ids.windows(2).any(|w| w[0] == w[1]) {
            return Err("two members have the same id".into());
        }
        for order in &self.orders {
            order_from_seed(order).ok_or("an order needs an id, a title and items with item and quantity")?;
        }
        Ok(())
    }

    fn member(&self, id: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.id == id)
    }
}

/// SHA-256 as lower-case hex.
pub fn hash(text: &str) -> String {
    ring::digest::digest(&ring::digest::SHA256, text.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// The seconds since 1970.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A user code as typed: upper case, without spaces, with the dash in the middle.
pub fn normal_code(typed: &str) -> String {
    let letters: String = typed.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_uppercase();
    if letters.len() == 8 { format!("{}-{}", &letters[..4], &letters[4..]) } else { letters }
}

#[derive(Clone, Debug)]
struct Grant {
    member: String,
    scopes: Vec<String>,
    expires: u64,
    family: String,
}

impl Grant {
    fn to_json(&self) -> Value {
        json!({ "member": self.member, "scopes": self.scopes, "expires": self.expires, "family": self.family })
    }

    fn from_json(value: &Value) -> Option<Grant> {
        Some(Grant {
            member: value.get("member")?.as_str()?.to_string(),
            scopes: value.get("scopes")?.as_array()?.iter().filter_map(Value::as_str).map(str::to_string).collect(),
            expires: value.get("expires")?.as_u64()?,
            family: value.get("family")?.as_str()?.to_string(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Decision {
    Open,
    Approved { member: String, scopes: Vec<String> },
    Denied,
}

#[derive(Clone, Debug)]
struct Pending {
    device_hash: String,
    user_code: String,
    scopes: Vec<String>,
    expires: u64,
    interval: u64,
    last_poll: Option<u64>,
    decision: Decision,
    tries: u32,
    /// After the member's key was right: the hash of a short-lived ticket and the member, so the page of the rights
    /// carries the ticket instead of the key.
    ticket: Option<(String, String)>,
}

#[derive(Default)]
struct State {
    access: HashMap<String, Grant>,
    refresh: HashMap<String, Grant>,
    pending: Vec<Pending>,
    stock: BTreeMap<String, Vec<Value>>,
    blueprints: BTreeMap<String, Vec<String>>,
    orders: Vec<Value>,
    /// Answers of deliveries by member and idempotency key.
    deliveries: BTreeMap<String, (u16, Value)>,
    /// Requests per token family: minute and count.
    rate: HashMap<String, (u64, u32)>,
    /// The last time each member used a token, in seconds since 1970.
    contact: BTreeMap<String, u64>,
    /// Every delivery booked: id, order, title, member, items, time.
    delivery_log: Vec<Value>,
    /// When the state file was last written.
    saved: u64,
    /// Refresh tokens already rotated, by hash: the family and when; a second use ends the whole login.
    rotated: HashMap<String, (String, u64)>,
    /// Wrong member keys on the panel over all codes: minute and count.
    panel_fails: (u64, u32),
    /// Sessions and the brake of the administration; never written to the file.
    admin: admin::AdminState,
    /// The people and the role map of the portal (in the file), its sessions and logins (in memory only).
    portal: portal::PortalState,
}

fn order_from_seed(seed: &Value) -> Option<Value> {
    let id = text_of(seed, "id")?;
    let title = text_of(seed, "title")?;
    let mut items = Vec::new();
    for item in seed.get("items")?.as_array()? {
        let name = text_of(item, "item")?;
        let quantity = item.get("quantity")?.as_f64().filter(|q| q.is_finite() && *q > 0.0)?;
        let mut entry = json!({ "item": name, "quantity": quantity, "delivered": 0.0 });
        if let Some(unit) = text_of(item, "unit") {
            entry["unit"] = json!(unit);
        }
        items.push(entry);
    }
    if items.is_empty() {
        return None;
    }
    Some(json!({ "id": id, "title": title, "status": "open", "items": items }))
}

fn grants_of(map: &HashMap<String, Grant>) -> Value {
    Value::Object(map.iter().map(|(k, g)| (k.clone(), g.to_json())).collect())
}

fn grants_from(value: Option<&Value>) -> HashMap<String, Grant> {
    value
        .and_then(Value::as_object)
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), Grant::from_json(v)?))).collect())
        .unwrap_or_default()
}

impl State {
    fn to_json(&self, now: u64) -> Value {
        let deliveries: Map<String, Value> =
            self.deliveries.iter().map(|(k, (s, b))| (k.clone(), json!({ "status": s, "body": b }))).collect();
        json!({
            "format": STATE_FORMAT,
            "saved": now,
            "access": grants_of(&self.access),
            "refresh": grants_of(&self.refresh),
            "stock": self.stock,
            "blueprints": self.blueprints,
            "orders": self.orders,
            "deliveries": deliveries,
            "contact": self.contact,
            "delivery_log": self.delivery_log,
            "rotated": self.rotated.iter().map(|(k, (f, at))| (k.clone(), json!({ "family": f, "at": at }))).collect::<Map<String, Value>>(),
            "portal": self.portal.to_json(),
        })
    }

    /// The state of a file in the current format; format 1 has no contacts, no delivery log and no time of saving,
    /// formats 1 and 2 have no portal; what is missing starts empty.
    fn from_json(root: &Value) -> State {
        let mut state = State {
            access: grants_from(root.get("access")),
            refresh: grants_from(root.get("refresh")),
            ..State::default()
        };
        if let Some(stock) = root.get("stock").and_then(Value::as_object) {
            for (member, items) in stock {
                state.stock.insert(member.clone(), items.as_array().cloned().unwrap_or_default());
            }
        }
        if let Some(blueprints) = root.get("blueprints").and_then(Value::as_object) {
            for (member, names) in blueprints {
                let names = names.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string);
                state.blueprints.insert(member.clone(), names.collect());
            }
        }
        state.orders = root.get("orders").and_then(Value::as_array).cloned().unwrap_or_default();
        if let Some(deliveries) = root.get("deliveries").and_then(Value::as_object) {
            for (key, entry) in deliveries {
                let status = entry.get("status").and_then(Value::as_u64).unwrap_or(201) as u16;
                state.deliveries.insert(key.clone(), (status, entry.get("body").cloned().unwrap_or(Value::Null)));
            }
        }
        if let Some(contact) = root.get("contact").and_then(Value::as_object) {
            state.contact = contact.iter().filter_map(|(m, t)| Some((m.clone(), t.as_u64()?))).collect();
        }
        state.delivery_log = root.get("delivery_log").and_then(Value::as_array).cloned().unwrap_or_default();
        state.saved = root.get("saved").and_then(Value::as_u64).unwrap_or(0);
        if let Some(rotated) = root.get("rotated").and_then(Value::as_object) {
            state.rotated = rotated
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), (v.get("family")?.as_str()?.to_string(), v.get("at")?.as_u64()?))))
                .collect();
        }
        state.portal = portal::PortalState::from_json(root.get("portal"));
        state
    }
}

/// `20270115-080000` of a time in seconds since 1970, for the folder of a backup.
fn stamp(seconds: u64) -> String {
    let (year, month, day) = status::date_of((seconds / 86_400) as i64);
    let second = seconds % 86_400;
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", second / 3600, second % 3600 / 60, second % 60)
}

/// Copies `file` into a new folder `backup-<time>` next to it and gives back that folder.
fn back_up(file: &Path, now: u64) -> Result<PathBuf, String> {
    let dir = file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let base = format!("backup-{}", stamp(now));
    let mut target = dir.join(&base);
    let mut n = 1;
    while target.exists() {
        n += 1;
        target = dir.join(format!("{base}-{n}"));
    }
    std::fs::create_dir(&target).map_err(|e| format!("cannot create the backup folder {}: {e}", target.display()))?;
    let name = file.file_name().unwrap_or_default();
    std::fs::copy(file, target.join(name)).map_err(|e| format!("cannot back up {}: {e}", file.display()))?;
    let _ = setup::make_private(&target.join(name));
    Ok(target)
}

/// The server.
pub struct Server {
    config: Config,
    state: Mutex<State>,
    rng: SystemRandom,
    /// When the server started, in seconds since 1970.
    started: u64,
    /// What the start did (data moved, backup made), for the log.
    notes: Vec<String>,
    /// Requests refused because they did not come over HTTPS.
    refused: AtomicU64,
    /// Whether the data folder took a probe file at the start.
    data_writable: bool,
    /// What the last update check found.
    update: Mutex<updates::UpdateState>,
    /// Only one admin password is checked at a time (PBKDF2 is slow on purpose).
    admin_gate: Mutex<()>,
    /// The way out to Discord for the portal; made on first use unless a test set another.
    outbound: Mutex<Option<Arc<dyn conformance::Exchange + Send + Sync>>>,
}

/// What a request may do after its token was checked.
struct Caller {
    member: String,
    scopes: Vec<String>,
}

impl Server {
    /// A server with the state from its file, or a fresh one with the orders of the configuration when there is no
    /// file yet. Data of an older format are moved after a backup into `backup-<time>/` next to the file. A file of a
    /// newer format, a broken one or one that cannot be read stops the start with the reason; nothing is overwritten.
    pub fn open(config: Config) -> Result<Server, String> {
        let now = unix_now();
        let mut notes = Vec::new();
        let mut data_writable = true;
        let stored = match &config.state_file {
            None => None,
            Some(file) => {
                let dir = file.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
                data_writable = setup::writable(dir);
                match std::fs::read_to_string(file) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(format!("cannot read {}: {error}", file.display())),
                    Ok(text) => {
                        let root: Value = serde_json::from_str(&text).map_err(|e| {
                            format!("{} is not valid JSON ({e}); restore a backup (backup-*/ next to it) or remove the file", file.display())
                        })?;
                        let format = root.get("format").and_then(Value::as_u64).ok_or_else(|| {
                            format!("{} has no format number; restore a backup (backup-*/ next to it)", file.display())
                        })?;
                        if format > STATE_FORMAT {
                            return Err(format!(
                                "{} has format {format}, this server version {} knows formats up to {STATE_FORMAT}. \
                                 Start the newer image again, or restore the backup backup-*/ the update made next to the file.",
                                file.display(),
                                env!("CARGO_PKG_VERSION"),
                            ));
                        }
                        if format < STATE_FORMAT {
                            let backup = back_up(file, now)?;
                            notes.push(format!(
                                "{} moved from format {format} to format {STATE_FORMAT}; the old file is kept in {}",
                                file.display(),
                                backup.display()
                            ));
                        }
                        Some((State::from_json(&root), format < STATE_FORMAT))
                    }
                }
            }
        };
        let moved = stored.as_ref().is_some_and(|(_, moved)| *moved);
        let state = stored.map(|(state, _)| state).unwrap_or_else(|| State {
            orders: config.orders.iter().filter_map(order_from_seed).collect(),
            ..State::default()
        });
        let server = Server {
            config,
            state: Mutex::new(state),
            rng: SystemRandom::new(),
            started: now,
            notes,
            refused: AtomicU64::new(0),
            data_writable,
            update: Mutex::new(updates::UpdateState::default()),
            admin_gate: Mutex::new(()),
            outbound: Mutex::new(None),
        };
        if moved {
            server.save(&server.lock());
        }
        Ok(server)
    }

    /// A server as [`Server::open`]; for tests and tools, where a state that cannot be opened is a mistake.
    pub fn new(config: Config) -> Server {
        Server::open(config).unwrap_or_else(|why| panic!("{why}"))
    }

    /// The same server with another way out to Discord; for tests, which answer in place of Discord.
    pub fn with_outbound(self, exchange: Arc<dyn conformance::Exchange + Send + Sync>) -> Server {
        *self.outbound.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(exchange);
        self
    }

    /// The way out to Discord.
    fn outbound(&self) -> Arc<dyn conformance::Exchange + Send + Sync> {
        self.outbound.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_or_insert_with(|| Arc::new(net::Client::new())).clone()
    }

    /// What the start did, for the log.
    pub fn notes(&self) -> &[String] {
        &self.notes
    }

    /// How many requests were refused because they did not come over HTTPS.
    pub fn refused_plain(&self) -> u64 {
        self.refused.load(Ordering::Relaxed)
    }

    /// Whether the data folder took a probe file at the start.
    pub fn data_writable(&self) -> bool {
        self.data_writable
    }

    /// When the state file was last written, in seconds since 1970; 0 before the first time.
    pub fn last_saved(&self) -> u64 {
        self.lock().saved
    }

    /// What the last update check found.
    pub fn update_state(&self) -> updates::UpdateState {
        self.update.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    /// Keeps what an update check found.
    pub fn set_update(&self, found: updates::UpdateState) {
        *self.update.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = found;
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// When the server started, in seconds since 1970.
    pub fn started(&self) -> u64 {
        self.started
    }

    /// How many members of the configuration hold a login (a refresh token).
    pub fn connected_members(&self) -> usize {
        let state = self.lock();
        let mut members: Vec<&str> = state
            .refresh
            .values()
            .map(|grant| grant.member.as_str())
            .filter(|member| self.config.member(member).is_some())
            .collect();
        members.sort_unstable();
        members.dedup();
        members.len()
    }

    /// Whether the portal has no Discord role mapped and nobody let in by hand.
    pub fn portal_unmapped(&self) -> bool {
        portal::unmapped(&self.lock())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Answers a request now.
    pub fn handle(&self, request: &Request) -> Response {
        self.handle_at(request, unix_now())
    }

    /// Answers a request at the time `now` (seconds since 1970): `/healthz` always, everything else outside the test
    /// mode only when the proxy in front says it came over HTTPS; every answer gets the security headers.
    pub fn handle_at(&self, request: &Request, now: u64) -> Response {
        let mut answer = if request.path == "/healthz" {
            if request.method == "GET" { Response::json(200, &json!({ "status": "ok" })) } else { not_allowed() }
        } else if self.config.require_https && !setup::forwarded_https(request) {
            self.refused.fetch_add(1, Ordering::Relaxed);
            Response::problem(403, "INVALID_REQUEST", "this server answers only over HTTPS, through the reverse proxy in front of it")
        } else {
            self.route(request, now)
        };
        harden(&mut answer, self.config.require_https);
        answer
    }

    fn route(&self, request: &Request, now: u64) -> Response {
        let method = request.method.as_str();
        let path = request.path.as_str();
        if admin::owns(path) {
            return admin::handle(self, request, now);
        }
        if portal::owns(path) {
            return portal::handle(self, request, now);
        }
        if path == WELL_KNOWN {
            return if method == "GET" { self.discovery() } else { not_allowed() };
        }
        if path == "/" {
            return if method == "GET" { status::page(self, request, now) } else { not_allowed() };
        }
        if let Some(asset) = look::asset(path) {
            return if method == "GET" { asset } else { not_allowed() };
        }
        if path == PANEL_PATH {
            let mut state = self.lock();
            state.pending.retain(|p| p.expires > now);
            return panel::page(self, &mut state, request, now);
        }
        let Some(route) = path.strip_prefix(API_PATH) else {
            return Response::problem(404, "NOT_FOUND", "no such address");
        };
        let delivery = route
            .strip_prefix("/orders/")
            .and_then(|rest| rest.strip_suffix("/deliveries"))
            .filter(|id| !id.is_empty() && !id.contains('/'))
            .map(http::percent_decode);
        match (method, route) {
            ("POST", "/device/code") => self.device_code(request, now),
            ("POST", "/token") => self.token(request, now),
            ("POST", "/revoke") => self.revoke(request),
            ("GET", "/me") => self.api(request, now, None, |s, _, c| s.me(c)),
            ("GET", "/stock") => self.api(request, now, Some("stock:read"), |s, st, _| s.stock(st, request)),
            ("PUT", "/stock/mine") => self.api(request, now, Some("stock:write"), |s, st, c| s.put_stock(st, c, request)),
            ("GET", "/blueprints") => self.api(request, now, Some("blueprints:read"), |s, st, _| s.blueprints(st, request)),
            ("PUT", "/blueprints/mine") => {
                self.api(request, now, Some("blueprints:write"), |s, st, c| s.put_blueprints(st, c, request))
            }
            ("GET", "/orders") => self.api(request, now, Some("orders:read"), |_, st, _| page(&st.orders, request, "orders")),
            ("POST", _) if delivery.is_some() => {
                let id = delivery.unwrap_or_default();
                self.api(request, now, Some("orders:write"), |s, st, c| s.deliver(st, c, &id, request, now))
            }
            (_, _) if delivery.is_some() => not_allowed(),
            (
                _,
                "/device/code" | "/token" | "/revoke" | "/me" | "/stock" | "/stock/mine" | "/blueprints"
                | "/blueprints/mine" | "/orders",
            ) => not_allowed(),
            _ => Response::problem(404, "NOT_FOUND", "no such address"),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.public_url)
    }

    fn discovery(&self) -> Response {
        let mut org = json!({ "name": self.config.name, "tag": self.config.tag });
        if let Some(homepage) = &self.config.homepage {
            org["homepage"] = json!(homepage);
        }
        Response::json(
            200,
            &json!({
                "standard": STANDARD,
                "org": org,
                "server": { "name": "versekit-orgserver", "version": env!("CARGO_PKG_VERSION") },
                "versions": {
                    VERSION: {
                        "api_base": self.url(API_PATH),
                        "device_authorization_endpoint": self.url(&format!("{API_PATH}/device/code")),
                        "token_endpoint": self.url(&format!("{API_PATH}/token")),
                        "revocation_endpoint": self.url(&format!("{API_PATH}/revoke")),
                        "verification_uri": self.url(PANEL_PATH),
                        "scopes_supported": self.config.scopes_offered,
                    }
                }
            }),
        )
        .with_header("Access-Control-Allow-Origin", "*")
    }

    fn random_bytes(&self, bytes: usize) -> Vec<u8> {
        let mut buffer = vec![0u8; bytes];
        if self.rng.fill(&mut buffer).is_err() {
            panic!("the system gives no random numbers");
        }
        buffer
    }

    fn random_hex(&self, bytes: usize) -> String {
        self.random_bytes(bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    fn user_code(&self) -> String {
        let mut letters = String::new();
        while letters.len() < 8 {
            let mut byte = [0u8; 1];
            if self.rng.fill(&mut byte).is_err() {
                panic!("the system gives no random numbers");
            }
            // 240 is the largest multiple of 20 below 256: no letter comes up more often.
            if byte[0] < 240 {
                letters.push(CODE_LETTERS[(byte[0] % 20) as usize] as char);
            }
        }
        normal_code(&letters)
    }

    fn device_code(&self, request: &Request, now: u64) -> Response {
        let form = request.form();
        let requested: Vec<String> = value(&form, "scope")
            .unwrap_or_default()
            .split(' ')
            .filter(|s| self.config.scopes_offered.contains(s))
            .map(str::to_string)
            .collect();
        if requested.is_empty() {
            return Response::oauth_error("invalid_scope");
        }
        let device_code = self.random_hex(32);
        let mut state = self.lock();
        state.pending.retain(|p| p.expires > now);
        if state.pending.len() >= PENDING_MAX {
            return Response::problem(429, "RATE_LIMITED", "too many logins are waiting; try again in a minute")
                .with_header("Retry-After", "60");
        }
        let user_code = loop {
            let code = self.user_code();
            if !state.pending.iter().any(|p| p.user_code == code) {
                break code;
            }
        };
        state.pending.push(Pending {
            device_hash: hash(&device_code),
            user_code: user_code.clone(),
            scopes: requested,
            expires: now + CODE_SECONDS,
            interval: self.config.poll_interval,
            last_poll: None,
            decision: Decision::Open,
            tries: 0,
            ticket: None,
        });
        Response::json(
            200,
            &json!({
                "device_code": device_code,
                "user_code": user_code,
                "verification_uri": self.url(PANEL_PATH),
                "expires_in": CODE_SECONDS,
                "interval": self.config.poll_interval,
            }),
        )
    }

    fn issue(&self, state: &mut State, member: &str, scopes: &[String], family: &str, now: u64) -> Response {
        let access = format!("vka_{}", self.random_hex(32));
        let refresh = format!("vkr_{}", self.random_hex(32));
        let grant = |expires| Grant { member: member.to_string(), scopes: scopes.to_vec(), expires, family: family.into() };
        state.access.retain(|_, g| g.expires > now);
        state.refresh.retain(|_, g| g.expires == 0 || g.expires > now);
        state.rotated.retain(|_, (_, at)| *at + REFRESH_IDLE > now);
        state.access.insert(hash(&access), grant(now + ACCESS_SECONDS));
        state.refresh.insert(hash(&refresh), grant(now + REFRESH_IDLE));
        state.contact.insert(member.to_string(), now);
        self.save(state);
        Response::json(
            200,
            &json!({
                "access_token": access,
                "token_type": "Bearer",
                "expires_in": ACCESS_SECONDS,
                "refresh_token": refresh,
                "scope": scopes.join(" "),
            }),
        )
    }

    fn token(&self, request: &Request, now: u64) -> Response {
        let form = request.form();
        let mut state = self.lock();
        match value(&form, "grant_type") {
            Some(DEVICE_GRANT) => {
                let wanted = hash(value(&form, "device_code").unwrap_or_default());
                let Some(index) = state.pending.iter().position(|p| p.device_hash == wanted) else {
                    return Response::oauth_error("invalid_grant");
                };
                if state.pending[index].expires <= now {
                    state.pending.remove(index);
                    return Response::oauth_error("expired_token");
                }
                let pending = &mut state.pending[index];
                match pending.decision.clone() {
                    Decision::Denied => {
                        state.pending.remove(index);
                        Response::oauth_error("access_denied")
                    }
                    Decision::Approved { member, scopes } => {
                        state.pending.remove(index);
                        let family = self.random_hex(16);
                        self.issue(&mut state, &member, &scopes, &family, now)
                    }
                    Decision::Open => {
                        let early = pending.last_poll.is_some_and(|last| now < last + pending.interval);
                        pending.last_poll = Some(now);
                        if early {
                            pending.interval += 5;
                            Response::oauth_error("slow_down")
                        } else {
                            Response::oauth_error("authorization_pending")
                        }
                    }
                }
            }
            Some("refresh_token") => {
                let old = hash(value(&form, "refresh_token").unwrap_or_default());
                let Some(grant) = state.refresh.remove(&old) else {
                    // A refresh token used a second time after its rotation was copied: the whole login ends.
                    if let Some((family, _)) = state.rotated.remove(&old) {
                        state.refresh.retain(|_, g| g.family != family);
                        state.access.retain(|_, g| g.family != family);
                        self.save(&state);
                    }
                    return Response::oauth_error("invalid_grant");
                };
                if self.config.member(&grant.member).is_none() || (grant.expires != 0 && grant.expires <= now) {
                    self.save(&state);
                    return Response::oauth_error("invalid_grant");
                }
                state.access.retain(|_, g| g.family != grant.family);
                state.rotated.insert(old, (grant.family.clone(), now));
                self.issue(&mut state, &grant.member, &grant.scopes, &grant.family, now)
            }
            _ => Response::oauth_error("unsupported_grant_type"),
        }
    }

    fn revoke(&self, request: &Request) -> Response {
        let form = request.form();
        let wanted = hash(value(&form, "token").unwrap_or_default());
        let mut state = self.lock();
        let family = state.refresh.get(&wanted).or_else(|| state.access.get(&wanted)).map(|g| g.family.clone());
        if let Some(family) = family {
            state.refresh.retain(|_, g| g.family != family);
            state.access.retain(|_, g| g.family != family);
            self.save(&state);
        }
        Response::json(200, &json!({}))
    }

    /// Checks the token, the rate and the right, then runs `act` under the lock.
    fn api(
        &self,
        request: &Request,
        now: u64,
        scope: Option<&str>,
        act: impl FnOnce(&Server, &mut State, &Caller) -> Response,
    ) -> Response {
        let token = request
            .header("Authorization")
            .and_then(|h| h.split_once(' '))
            .filter(|(kind, _)| kind.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| token.trim())
            .unwrap_or_default();
        let mut state = self.lock();
        let grant = state.access.get(&hash(token)).filter(|g| g.expires > now).cloned();
        let Some(grant) = grant.filter(|g| self.config.member(&g.member).is_some()) else {
            return Response::problem(401, "UNAUTHORIZED", "token missing, unknown or expired")
                .with_header("WWW-Authenticate", "Bearer error=\"invalid_token\"");
        };
        // The last contact is kept to the minute: the file is written at most once a minute for it.
        let before = state.contact.insert(grant.member.clone(), now);
        if before.is_none_or(|at| at / 60 != now / 60) {
            self.save(&state);
        }
        let minute = now / 60;
        let entry = state.rate.entry(grant.family.clone()).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        entry.1 += 1;
        if entry.1 > self.config.rate_limit {
            return Response::problem(429, "RATE_LIMITED", "too many requests")
                .with_header("Retry-After", &(60 - now % 60).to_string());
        }
        if let Some(scope) = scope
            && !grant.scopes.iter().any(|s| s == scope)
        {
            return Response::problem(403, "SCOPE_MISSING", scope);
        }
        let caller = Caller { member: grant.member, scopes: grant.scopes };
        act(self, &mut state, &caller)
    }

    fn me(&self, caller: &Caller) -> Response {
        let name = self.config.member(&caller.member).map(|m| m.name.as_str()).unwrap_or_default();
        Response::json(
            200,
            &json!({
                "member": { "id": caller.member, "name": name },
                "org": { "name": self.config.name, "tag": self.config.tag },
                "scopes": caller.scopes,
            }),
        )
    }

    fn member_name(&self, id: &str) -> String {
        self.config.member(id).map_or_else(|| id.to_string(), |m| m.name.clone())
    }

    fn stock(&self, state: &State, request: &Request) -> Response {
        let mut items = Vec::new();
        for (member, list) in &state.stock {
            for item in list {
                let mut item = item.clone();
                item["member"] = json!(self.member_name(member));
                items.push(item);
            }
        }
        page(&items, request, "items")
    }

    fn put_stock(&self, state: &mut State, caller: &Caller, request: &Request) -> Response {
        let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
            return Response::problem(400, "INVALID_BODY", "body is not JSON");
        };
        let Some(list) = body.get("items").and_then(Value::as_array).filter(|l| l.len() <= MAX_ITEMS) else {
            return Response::problem(400, "INVALID_BODY", "items must be a list of at most 5000 entries");
        };
        let Some(items) = list.iter().map(stock_item).collect::<Option<Vec<_>>>() else {
            return Response::problem(400, "INVALID_BODY", "an item lacks item or quantity, or a value is out of range");
        };
        let count = items.len();
        state.stock.insert(caller.member.clone(), items);
        self.save(state);
        Response::json(200, &json!({ "items_stored": count }))
    }

    fn blueprints(&self, state: &State, request: &Request) -> Response {
        let mut holders: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for (member, names) in &state.blueprints {
            for name in names {
                holders.entry(name).or_default().push(self.member_name(member));
            }
        }
        let list: Vec<Value> = holders.into_iter().map(|(name, h)| json!({ "name": name, "holders": h })).collect();
        page(&list, request, "blueprints")
    }

    fn put_blueprints(&self, state: &mut State, caller: &Caller, request: &Request) -> Response {
        let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
            return Response::problem(400, "INVALID_BODY", "body is not JSON");
        };
        let names = body.get("blueprints").and_then(Value::as_array).filter(|l| l.len() <= MAX_BLUEPRINTS).and_then(
            |list| {
                list.iter()
                    .map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty() && s.chars().count() <= MAX_TEXT))
                    .map(|s| s.map(str::to_string))
                    .collect::<Option<Vec<String>>>()
            },
        );
        let Some(mut names) = names else {
            return Response::problem(400, "INVALID_BODY", "blueprints must be a list of names");
        };
        names.sort();
        names.dedup();
        let count = names.len();
        state.blueprints.insert(caller.member.clone(), names);
        self.save(state);
        Response::json(200, &json!({ "blueprints_stored": count }))
    }

    fn deliver(&self, state: &mut State, caller: &Caller, id: &str, request: &Request, now: u64) -> Response {
        let Some(key) = request.header("Idempotency-Key").map(str::trim).filter(|k| !k.is_empty() && k.len() <= 200)
        else {
            return Response::problem(400, "IDEMPOTENCY_KEY_MISSING", "send an Idempotency-Key header");
        };
        let slot = format!("{}\n{key}", caller.member);
        if let Some((status, body)) = state.deliveries.get(&slot) {
            return Response::json(*status, body);
        }
        let Ok(body) = serde_json::from_slice::<Value>(&request.body) else {
            return Response::problem(400, "INVALID_BODY", "body is not JSON");
        };
        let wanted: Option<Vec<(String, f64)>> = body.get("items").and_then(Value::as_array).and_then(|list| {
            list.iter()
                .map(|i| {
                    Some((
                        text_of(i, "item")?.to_string(),
                        i.get("quantity")?.as_f64().filter(|q| q.is_finite() && *q > 0.0 && *q <= QUANTITY_MAX)?,
                    ))
                })
                .collect()
        });
        let Some(wanted) = wanted.filter(|w| !w.is_empty()) else {
            return Response::problem(400, "INVALID_BODY", "items must list item and a quantity above 0");
        };
        let Some(order) = state.orders.iter_mut().find(|o| o.get("id").and_then(Value::as_str) == Some(id)) else {
            return Response::problem(404, "NOT_FOUND", "no such order");
        };
        if order.get("status").and_then(Value::as_str) != Some("open") {
            return Response::problem(409, "ORDER_CLOSED", "the order is no longer open");
        }
        let Some(lines) = order.get_mut("items").and_then(Value::as_array_mut) else {
            return Response::problem(500, "INTERNAL", "order without items");
        };
        let mut seats = Vec::new();
        for (item, _) in &wanted {
            match lines.iter().position(|l| l.get("item").and_then(Value::as_str) == Some(item.as_str())) {
                Some(seat) => seats.push(seat),
                None => return Response::problem(422, "ITEM_NOT_IN_ORDER", item),
            }
        }
        for ((_, quantity), seat) in wanted.iter().zip(seats) {
            let delivered = lines[seat].get("delivered").and_then(Value::as_f64).unwrap_or(0.0) + quantity;
            lines[seat]["delivered"] = json!(delivered);
        }
        let complete = lines.iter().all(|l| {
            l.get("delivered").and_then(Value::as_f64).unwrap_or(0.0)
                >= l.get("quantity").and_then(Value::as_f64).unwrap_or(0.0)
        });
        if complete {
            order["status"] = json!("done");
        }
        let title = order.get("title").cloned().unwrap_or(Value::Null);
        let delivery_id = self.random_hex(8);
        let answer = json!({ "delivery_id": delivery_id, "order": order.clone() });
        let items: Vec<Value> = wanted.iter().map(|(item, quantity)| json!({ "item": item, "quantity": quantity })).collect();
        state.delivery_log.push(json!({
            "id": delivery_id,
            "order": id,
            "title": title,
            "member": caller.member,
            "items": items,
            "at": now,
        }));
        if state.delivery_log.len() > DELIVERY_LOG_MAX {
            let extra = state.delivery_log.len() - DELIVERY_LOG_MAX;
            state.delivery_log.drain(..extra);
        }
        state.deliveries.insert(slot, (201, answer.clone()));
        self.save(state);
        Response::json(201, &answer)
    }

    /// Writes the state file through a side file, readable for the owner only; an error goes to stderr without
    /// content.
    fn save(&self, state: &State) {
        let Some(file) = &self.config.state_file else { return };
        let side = file.with_extension("tmp");
        let text = state.to_json(unix_now()).to_string();
        let result = std::fs::write(&side, text)
            .and_then(|()| setup::make_private(&side))
            .and_then(|()| std::fs::rename(&side, file));
        if let Err(error) = result {
            eprintln!("state file not written: {error}");
        }
    }

    /// Confirms a waiting login as the panel does after the member's key; for
    /// tests and tools. `scopes` of `None` grants every right asked for.
    pub fn approve(&self, user_code: &str, member: &str, scopes: Option<&[&str]>) -> bool {
        if self.config.member(member).is_none() {
            return false;
        }
        let mut state = self.lock();
        let Some(pending) = state.pending.iter_mut().find(|p| p.user_code == normal_code(user_code)) else {
            return false;
        };
        let granted = match scopes {
            Some(list) => pending.scopes.iter().filter(|s| list.contains(&s.as_str())).cloned().collect(),
            None => pending.scopes.clone(),
        };
        pending.decision = Decision::Approved { member: member.to_string(), scopes: granted };
        true
    }

    /// Refuses a waiting login; for tests and tools.
    pub fn deny(&self, user_code: &str) -> bool {
        let mut state = self.lock();
        let Some(pending) = state.pending.iter_mut().find(|p| p.user_code == normal_code(user_code)) else {
            return false;
        };
        pending.decision = Decision::Denied;
        true
    }

    /// The user codes of the logins still waiting.
    pub fn waiting_codes(&self) -> Vec<String> {
        self.lock().pending.iter().filter(|p| p.decision == Decision::Open).map(|p| p.user_code.clone()).collect()
    }

    /// How many refresh tokens are valid.
    pub fn refresh_count(&self) -> usize {
        self.lock().refresh.len()
    }

    /// The stock shared by `member`.
    pub fn stock_of(&self, member: &str) -> Vec<Value> {
        self.lock().stock.get(member).cloned().unwrap_or_default()
    }

    /// Makes every access token run out, as if an hour had passed.
    pub fn expire_access(&self) {
        self.lock().access.values_mut().for_each(|g| g.expires = 0);
    }

    fn member_by_key(&self, key: &str) -> Option<&Member> {
        let wanted = hash(key);
        self.config.members.iter().find(|m| m.key_hash == wanted)
    }
}

fn not_allowed() -> Response {
    Response::problem(405, "METHOD_NOT_ALLOWED", "this address does not take that method")
}

/// The security headers every answer carries; a page sets its own Content-Security-Policy, any other answer gets one
/// that allows nothing. HSTS only where HTTPS is required.
fn harden(answer: &mut Response, https: bool) {
    let mut add = |name: &str, value: &str| {
        if !answer.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
            answer.headers.push((name.to_string(), value.to_string()));
        }
    };
    add("X-Content-Type-Options", "nosniff");
    add("Referrer-Policy", "same-origin");
    add("X-Frame-Options", "DENY");
    add("Content-Security-Policy", "default-src 'none'; frame-ancestors 'none'");
    add("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
    if https {
        add("Strict-Transport-Security", "max-age=15552000");
    }
}

fn stock_item(value: &Value) -> Option<Value> {
    let item = text_of(value, "item").filter(|s| s.chars().count() <= MAX_TEXT)?;
    let quantity = value.get("quantity")?.as_f64().filter(|q| q.is_finite() && *q >= 0.0 && *q <= QUANTITY_MAX)?;
    let mut out = json!({ "item": item, "quantity": quantity });
    if let Some(unit) = value.get("unit") {
        out["unit"] = json!(unit.as_str().filter(|u| u.len() <= 20)?);
    }
    if let Some(quality) = value.get("quality") {
        out["quality"] = json!(quality.as_u64().filter(|q| *q <= 1000)?);
    }
    if let Some(location) = value.get("location") {
        out["location"] = json!(location.as_str().filter(|l| l.chars().count() <= MAX_TEXT)?);
    }
    Some(out)
}

/// One page of `items` after `cursor`, with `next_cursor` while more follow.
fn page(items: &[Value], request: &Request, key: &str) -> Response {
    let query = request.query_pairs();
    let start = match value(&query, "cursor") {
        None => 0,
        Some(text) => match text.parse::<usize>() {
            Ok(n) if n <= items.len() => n,
            _ => return Response::problem(400, "INVALID_CURSOR", "the cursor is unknown"),
        },
    };
    let limit = value(&query, "limit").and_then(|l| l.parse::<usize>().ok()).unwrap_or(PAGE_DEFAULT).clamp(1, PAGE_MAX);
    let end = (start + limit).min(items.len());
    let next = if end < items.len() { json!(end.to_string()) } else { Value::Null };
    Response::json(200, &json!({ key: items[start..end], "next_cursor": next }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_origins_or_loopback_http_serve() {
        for good in ["https://orga.example.org", "https://orga.example.org:8443", "http://127.0.0.1:8080", "http://localhost"]
        {
            assert!(origin_ok(good), "{good}");
        }
        for bad in [
            "http://orga.example.org",
            "https://orga.example.org/panel",
            "https://user@orga.example.org", // privacy-ok: reserved example domain
            "ftp://orga.example.org",
            "orga.example.org",
            "https://",
            "https://orga.example.org:x",
        ] {
            assert!(!origin_ok(bad), "{bad}");
        }
    }

    #[test]
    fn the_configuration_is_read_and_checked() {
        let good = r#"{"public_url": "https://orga.example.org/", "name": "Orga", "tag": "ORG",
            "members": [{"id": "m1", "name": "Pilot", "key": "0123456789abcdef"}],
            "orders": [{"id": "a1", "title": "Erz", "items": [{"item": "Quantanium", "quantity": 4, "unit": "scu"}]}]}"#;
        let config = Config::from_json(good).unwrap();
        assert_eq!(config.public_url, "https://orga.example.org");
        assert_eq!(config.members[0].key_hash, hash("0123456789abcdef"));
        let short_key = good.replace("0123456789abcdef", "kurz");
        assert!(Config::from_json(&short_key).unwrap_err().contains("shorter"));
        let plain = good.replace("https://orga", "http://orga");
        assert!(Config::from_json(&plain).is_err());
        let bad_order = good.replace("\"quantity\": 4", "\"quantity\": 0");
        assert!(Config::from_json(&bad_order).is_err());
    }

    #[test]
    fn user_codes_have_the_shape_of_the_standard() {
        let server = Server::new(Config::new("http://127.0.0.1:1", "O", "O"));
        for _ in 0..50 {
            let code = server.user_code();
            assert_eq!(code.len(), 9);
            assert_eq!(&code[4..5], "-");
            assert!(code.chars().filter(|c| *c != '-').all(|c| CODE_LETTERS.contains(&(c as u8))), "{code}");
        }
        assert_eq!(normal_code(" bcdf ghjk "), "BCDF-GHJK");
    }
}
