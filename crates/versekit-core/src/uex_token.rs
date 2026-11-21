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

//! The player's own UEX token, voluntary: set, every request to the UEX API carries
//! `Authorization: Bearer <token>`; not set, every request goes out as before.
//!
//! * Kept only in the system's store of secrets ([`SystemSecrets`], name [`SECRET`]), the same as the key of
//!   the Basetool: never in the data folder, the settings, the backup or the error report.
//! * Sent only to addresses below [`API_PREFIX`]; the agent drops it on a redirect.
//! * UEX refuses it (401, or 403 where the same request without it gets through): the requests go on without it,
//!   the page says so, and one note goes into the error log, not one per request.
//! * A copy of the data (`--daten`) never loads it, like the Basetool.

use std::fmt;
use std::sync::RwLock;

use crate::basetool::secrets::{SecretError, SecretStore, SystemSecrets};
use crate::errors;
use crate::paths::Paths;
use crate::scmdb::FetchError;

/// The name of the token in the store of secrets.
pub const SECRET: &str = "uex-token";

/// Only addresses starting with this get the token.
pub const API_PREFIX: &str = "https://api.uexcorp.uk/";

/// The address the button "Verbindung testen" asks: small, and cached long by UEX.
pub const CHECK_URL: &str = "https://api.uexcorp.uk/2.0/game_versions";

/// The longest token taken.
const MAX_LEN: usize = 512;
/// The shortest token taken: a shorter one would make the redaction replace common words.
const MIN_LEN: usize = 16;

/// The place in the error log of a refused token.
pub const NOTE_REFUSED: &str = "uex_token.abgelehnt";

/// A token as typed by the player, cleaned. Neither `Debug` nor any text of the program shows it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<geheim>)")
    }
}

impl Token {
    /// The token in `input`: spaces around it and a leading `Bearer ` are dropped. `None` for an empty input or one
    /// that cannot stand in a header line (spaces or control characters inside, non-ASCII, longer than 512), and one
    /// shorter than 16 characters.
    pub fn parse(input: &str) -> Option<Token> {
        let text = input.trim();
        let text = match text.get(..7) {
            Some(head) if head.eq_ignore_ascii_case("bearer ") => text[7..].trim_start(),
            _ => text,
        };
        let fits = (MIN_LEN..=MAX_LEN).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_graphic());
        fits.then(|| Token(text.to_string()))
    }

    fn reveal(&self) -> &str {
        &self.0
    }
}

struct State {
    token: Option<Token>,
    /// UEX refused the token: the requests go out without it until another one is set.
    refused: bool,
}

static STATE: RwLock<State> = RwLock::new(State { token: None, refused: false });

fn state() -> std::sync::RwLockReadGuard<'static, State> {
    STATE.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn state_mut() -> std::sync::RwLockWriteGuard<'static, State> {
    STATE.write().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Sets the token of the running program (`None`: none); a refusal of an earlier one is forgotten.
pub fn activate(token: Option<Token>) {
    let mut state = state_mut();
    state.token = token;
    state.refused = false;
}

/// The token that is set, for the button "Verbindung testen"; it shows as no text.
pub fn current() -> Option<Token> {
    state().token.clone()
}

/// Whether a token is set.
pub fn is_set() -> bool {
    state().token.is_some()
}

/// Whether UEX refused the token that is set.
pub fn refused() -> bool {
    let state = state();
    state.token.is_some() && state.refused
}

/// The token as it is set, refused or not, for the redaction of texts only.
pub(crate) fn secret_for_redaction() -> Option<String> {
    state().token.as_ref().map(|token| token.reveal().to_string())
}

/// The token for a request to `url`: only below [`API_PREFIX`], and not once UEX refused it.
fn bearer_for(url: &str) -> Option<Token> {
    let state = state();
    if state.refused || !url.starts_with(API_PREFIX) {
        return None;
    }
    state.token.clone()
}

/// Marks `token` as refused, if it is still the one set; notes it once.
fn refuse(token: &Token) {
    let newly = {
        let mut state = state_mut();
        let current = state.token.as_ref() == Some(token) && !state.refused;
        if current {
            state.refused = true;
        }
        current
    };
    if newly {
        errors::note(NOTE_REFUSED, &"UEX refused the token; the requests go on without it");
    }
}

/// One request: the address and the token to send with it, if any. A failure carries the HTTP status (0 where no
/// answer came).
pub type Send<'a> = &'a (dyn Fn(&str, Option<&str>) -> Result<Vec<u8>, (u16, FetchError)> + Sync);

/// Whether a request that failed with `status` speaks against the token.
fn against_token(status: u16) -> bool {
    matches!(status, 401 | 403)
}

/// Fetches `url` through `send`, with the token where one is set and the address is UEX's. Refused for the token
/// (401, or 403 where the same request without it gets through): asked again without it, and the token counts as
/// refused from now on.
pub fn fetch_through(url: &str, send: Send<'_>) -> Result<Vec<u8>, FetchError> {
    let Some(token) = bearer_for(url) else {
        return send(url, None).map_err(|(_, error)| error);
    };
    match send(url, Some(token.reveal())) {
        Err((status, _)) if against_token(status) => {
            let plain = send(url, None);
            if status == 401 || plain.is_ok() {
                refuse(&token);
            }
            plain.map_err(|(_, error)| error)
        }
        other => other.map_err(|(_, error)| error),
    }
}

/// What the button "Verbindung testen" found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    /// UEX answered the request with the token.
    Accepted,
    /// UEX refused the token.
    Refused,
    /// Fetching is switched off.
    Offline,
    /// No usable answer, with what it was.
    Failed(String),
}

/// Asks [`CHECK_URL`] with `token`. UEX does not say whether a token is valid where a request also works without
/// one: [`Check::Accepted`] means the request with it was answered.
pub fn check(token: &Token, send: Send<'_>) -> Check {
    match send(CHECK_URL, Some(token.reveal())) {
        Ok(bytes) => {
            let ok = std::str::from_utf8(&bytes)
                .ok()
                .and_then(|text| crate::jsonio::loads(text).ok())
                .is_some_and(|value| crate::stock_data::text_of(value.get("status")) == "ok");
            if ok { Check::Accepted } else { Check::Failed("unexpected answer".to_string()) }
        }
        Err((_, FetchError::Offline)) => Check::Offline,
        Err((status, error)) if against_token(status) => {
            if status == 401 || send(CHECK_URL, None).is_ok() {
                Check::Refused
            } else {
                Check::Failed(error.to_string())
            }
        }
        Err((_, error)) => Check::Failed(error.to_string()),
    }
}

/// [`check`] against the real UEX, named `user_agent`.
pub fn check_live(token: &Token, user_agent: &str) -> Check {
    if crate::net::offline() {
        return Check::Offline;
    }
    let agent = crate::scmdb::agent(user_agent);
    check(token, &|url, bearer| crate::scmdb::fetch_answer(&agent, url, bearer))
}

/// The stored token, or `None` without one, for a copy of the data, or where the store fails (reported).
pub fn load(paths: &Paths, secrets: &dyn SecretStore) -> Option<Token> {
    if crate::testmode::on_copy(paths) {
        return None;
    }
    match secrets.load(SECRET) {
        Ok(text) => text.as_deref().and_then(Token::parse),
        Err(error) => {
            errors::record("uex_token.load", &error);
            None
        }
    }
}

/// Loads the stored token of the player into the running program: at the start of each process on the own data.
pub fn start(paths: &Paths) {
    activate(load(paths, &SystemSecrets::from_host(&paths.host)));
}

/// Stores `token` and sets it for the running program.
///
/// # Errors
///
/// When the store refuses; the token set before stays.
pub fn save(secrets: &dyn SecretStore, token: &Token) -> Result<(), SecretError> {
    secrets.save(SECRET, token.reveal())?;
    activate(Some(token.clone()));
    Ok(())
}

/// Deletes the stored token; the requests go out without one from now on.
///
/// # Errors
///
/// When the store cannot delete it; the program uses none from now on all the same.
pub fn forget(secrets: &dyn SecretStore) -> Result<(), SecretError> {
    activate(None);
    secrets.delete(SECRET)
}

#[cfg(test)]
mod tests;
