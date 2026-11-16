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

//! What the orgs shared (stock, blueprints, orders), fetched in a thread of
//! its own and held **in memory only**: it is the data of other members, the
//! data folder is backed up.
//!
//! One fetch per org at a time: the server swaps the refresh token with every
//! refresh, so a second request on the old one would get `401`. The tokens
//! are stored after the fetch, also after an error, but only into the entry
//! that is still there and still the one the fetch started with.

use std::sync::{Arc, Mutex};

use crate::orgstd::data::{OrgBlueprint, OrgLot, OrgOrder, Parsed, parse_blueprints, parse_lots, parse_orders};
use crate::orgstd::discovery::Origin;
use crate::orgstd::store::Connection;
use crate::orgstd::{E_GONE, E_OFFLINE, E_RATE, E_STORE, OrgError};

use super::{Hub, lock};

/// The age from which a page that needs the data fetches it again when it opens, in seconds.
pub const MAX_AGE: u64 = 600;
/// The pause after a failed fetch, in seconds; a `429` names its own.
pub const PAUSE: u64 = 60;

/// Which list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Stock,
    Blueprints,
    Orders,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Stock, Kind::Blueprints, Kind::Orders];

    /// The right the list needs.
    pub fn scope(self) -> &'static str {
        match self {
            Kind::Stock => "stock:read",
            Kind::Blueprints => "blueprints:read",
            Kind::Orders => "orders:read",
        }
    }
}

/// One list of one org as it was last fetched.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot<T> {
    /// The rows of the last good fetch; they stay when a later one fails.
    pub rows: Arc<Vec<T>>,
    /// Rows of the last good fetch that broke the standard and were left out.
    pub dropped: usize,
    /// Seconds since 1970 of the last good fetch.
    pub fetched_at: Option<u64>,
    /// Why the last fetch failed.
    pub error: Option<OrgError>,
    /// No new fetch before this time (seconds since 1970).
    pub retry_at: u64,
    /// A fetch runs.
    pub loading: bool,
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Slot { rows: Arc::new(Vec::new()), dropped: 0, fetched_at: None, error: None, retry_at: 0, loading: false }
    }
}

/// What a page needs of a [`Slot`] to say how the list stands, without its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotInfo {
    pub fetched_at: Option<u64>,
    pub error: Option<OrgError>,
    pub retry_at: u64,
    pub loading: bool,
    pub dropped: usize,
}

impl<T> Slot<T> {
    /// How the list stands.
    pub fn info(&self) -> SlotInfo {
        SlotInfo {
            fetched_at: self.fetched_at,
            error: self.error,
            retry_at: self.retry_at,
            loading: self.loading,
            dropped: self.dropped,
        }
    }

    /// Seconds still to wait before a new fetch.
    pub fn pause_left(&self, now: u64) -> u64 {
        self.retry_at.saturating_sub(now)
    }

    fn done(&mut self, parsed: Parsed<T>, now: u64) {
        *self = Slot { rows: Arc::new(parsed.rows), dropped: parsed.dropped, fetched_at: Some(now), ..Slot::default() };
    }

    fn failed(&mut self, error: OrgError, now: u64) {
        self.error = Some(error);
        self.loading = false;
        self.retry_at = now + if error.code == E_RATE { error.retry_after.unwrap_or(PAUSE) } else { PAUSE };
    }
}

/// The three lists of one org.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrgData {
    pub stock: Slot<OrgLot>,
    pub blueprints: Slot<OrgBlueprint>,
    pub orders: Slot<OrgOrder>,
}

impl OrgData {
    /// Whether any list is being fetched.
    pub fn loading(&self) -> bool {
        self.stock.loading || self.blueprints.loading || self.orders.loading
    }

    /// How the list `kind` stands.
    pub fn info(&self, kind: Kind) -> SlotInfo {
        match kind {
            Kind::Stock => self.stock.info(),
            Kind::Blueprints => self.blueprints.info(),
            Kind::Orders => self.orders.info(),
        }
    }

    fn state(&self, kind: Kind) -> (bool, Option<u64>, u64) {
        match kind {
            Kind::Stock => (self.stock.loading, self.stock.fetched_at, self.stock.retry_at),
            Kind::Blueprints => (self.blueprints.loading, self.blueprints.fetched_at, self.blueprints.retry_at),
            Kind::Orders => (self.orders.loading, self.orders.fetched_at, self.orders.retry_at),
        }
    }
}

/// What a fetch brought, by list.
enum Fetched {
    Stock(Parsed<OrgLot>),
    Blueprints(Parsed<OrgBlueprint>),
    Orders(Parsed<OrgOrder>),
}

impl Hub {
    /// The lock of the org's tokens; held for as long as they are in use.
    pub(super) fn net_lock(&self, id: &str) -> Arc<Mutex<()>> {
        lock(&self.inner.net_locks).entry(id.to_string()).or_default().clone()
    }

    /// What the orgs shared, as far as it was fetched; cheap to call every frame.
    pub fn data(&self, id: &str) -> OrgData {
        lock(&self.inner.data).get(id).cloned().unwrap_or_default()
    }

    /// Puts made-up data in place of a fetch, for the preview, which asks nobody.
    pub fn preload(&self, id: &str, data: OrgData) {
        lock(&self.inner.data).insert(id.to_string(), data);
        self.change(|_| {});
    }

    /// Forgets the data of orgs that are gone and the lists a right of which was taken back.
    pub(super) fn prune_data(&self) {
        let connections = lock(&self.inner.view).connections.clone();
        let mut data = lock(&self.inner.data);
        data.retain(|id, _| connections.iter().any(|c| &c.id == id));
        for connection in &connections {
            let Some(entry) = data.get_mut(&connection.id) else { continue };
            let granted = |kind: Kind| connection.scopes.iter().any(|s| s == kind.scope());
            if !granted(Kind::Stock) && !entry.stock.loading {
                entry.stock = Slot::default();
            }
            if !granted(Kind::Blueprints) && !entry.blueprints.loading {
                entry.blueprints = Slot::default();
            }
            if !granted(Kind::Orders) && !entry.orders.loading {
                entry.orders = Slot::default();
            }
        }
    }

    fn edit_slot(&self, id: &str, kind: Kind, edit: impl FnOnce(&mut OrgData, Kind)) {
        {
            let mut data = lock(&self.inner.data);
            edit(data.entry(id.to_string()).or_default(), kind);
        }
        self.change(|_| {});
    }

    /// Fetches the list unless one runs, the right is missing, the pause after a failure has not passed or the
    /// org is not connected. Returns whether a fetch was started. Nothing asks an org on another computer while
    /// fetching is switched off.
    pub fn refresh(&self, id: &str, kind: Kind) -> bool {
        let now = (self.inner.clock)();
        let granted =
            lock(&self.inner.view).connections.iter().any(|c| c.id == id && c.scopes.iter().any(|s| s == kind.scope()));
        if !granted {
            return false;
        }
        let (loading, _, retry_at) = self.data(id).state(kind);
        if loading || retry_at > now {
            return false;
        }
        if self.inner.offline_blocks(id) {
            self.edit_slot(id, kind, |data, kind| match kind {
                Kind::Stock => data.stock.failed(OrgError::new(E_OFFLINE), now),
                Kind::Blueprints => data.blueprints.failed(OrgError::new(E_OFFLINE), now),
                Kind::Orders => data.orders.failed(OrgError::new(E_OFFLINE), now),
            });
            return false;
        }
        self.edit_slot(id, kind, |data, kind| match kind {
            Kind::Stock => data.stock.loading = true,
            Kind::Blueprints => data.blueprints.loading = true,
            Kind::Orders => data.orders.loading = true,
        });
        let (hub, id) = (self.clone(), id.to_string());
        std::thread::Builder::new()
            .name("orga-abruf".into())
            .spawn(move || {
                let result = hub.fetch(&id, kind);
                let now = (hub.inner.clock)();
                hub.edit_slot(&id, kind, |data, kind| match (kind, result) {
                    (Kind::Stock, Ok(Fetched::Stock(parsed))) => data.stock.done(parsed, now),
                    (Kind::Blueprints, Ok(Fetched::Blueprints(parsed))) => data.blueprints.done(parsed, now),
                    (Kind::Orders, Ok(Fetched::Orders(parsed))) => data.orders.done(parsed, now),
                    (Kind::Stock, Err(error)) => data.stock.failed(error, now),
                    (Kind::Blueprints, Err(error)) => data.blueprints.failed(error, now),
                    (Kind::Orders, Err(error)) => data.orders.failed(error, now),
                    _ => {}
                });
                hub.prune_data();
            })
            .expect("org fetch thread");
        true
    }

    /// [`Hub::refresh`] when the list was never fetched or is older than `max_age` seconds; for the moment a
    /// page that needs it opens. Nothing calls this on a clock.
    pub fn ensure(&self, id: &str, kind: Kind, max_age: u64) -> bool {
        let now = (self.inner.clock)();
        let (_, fetched_at, _) = self.data(id).state(kind);
        if fetched_at.is_some_and(|at| now.saturating_sub(at) < max_age) {
            return false;
        }
        self.refresh(id, kind)
    }

    fn fetch(&self, id: &str, kind: Kind) -> Result<Fetched, OrgError> {
        self.call_org(id, |client, connection, now| match kind {
            Kind::Stock => client.org_stock(connection, now).map(|rows| Fetched::Stock(parse_lots(&rows))),
            Kind::Blueprints => {
                client.org_blueprints(connection, now).map(|rows| Fetched::Blueprints(parse_blueprints(&rows)))
            }
            Kind::Orders => client.orders(connection, now).map(|rows| Fetched::Orders(parse_orders(&rows))),
        })
    }

    /// Runs `work` on a copy of the connection of `id` under the lock of its tokens, then stores the tokens that
    /// changed on the way (also after an error), into the entry that is still there.
    pub(super) fn call_org<T>(
        &self,
        id: &str,
        work: impl FnOnce(&crate::orgstd::client::OrgClient, &mut Connection, u64) -> Result<T, OrgError>,
    ) -> Result<T, OrgError> {
        let net = self.net_lock(id);
        let _tokens = lock(&net);
        let mut connection = {
            let _store = lock(&self.inner.store_lock);
            let list = self.inner.store.load().map_err(|_| OrgError::new(E_STORE))?;
            list.into_iter().find(|c| c.id() == id).ok_or(OrgError::new(E_GONE))?
        };
        let before = connection.clone();
        let now = (self.inner.clock)();
        let result = work(&self.inner.client, &mut connection, now);
        self.keep_tokens(&before, &connection);
        result
    }

    /// Stores the tokens `after` a request went through, if they changed: only into the entry that still is the
    /// one `before` was read from. When it is gone or replaced, the new refresh token is given back.
    pub(super) fn keep_tokens(&self, before: &Connection, after: &Connection) {
        if before.refresh_token == after.refresh_token {
            return;
        }
        let _store = lock(&self.inner.store_lock);
        let mut list = self.inner.store.load().unwrap_or_default();
        let same = list.iter_mut().find(|c| c.id() == before.id());
        match same {
            Some(stored)
                if stored.refresh_token == before.refresh_token && stored.connected_at == before.connected_at =>
            {
                stored.access_token = after.access_token.clone();
                stored.refresh_token = after.refresh_token.clone();
                stored.access_expires = after.access_expires;
                stored.scopes = after.scopes.clone();
                if self.inner.store.save(&list).is_err() {
                    let _ = self.inner.client.revoke(&after.discovery, &after.refresh_token);
                }
            }
            _ => {
                let _ = self.inner.client.revoke(&after.discovery, &after.refresh_token);
            }
        }
    }
}

impl super::Inner {
    /// Whether nothing may be asked of the org: fetching is switched off and it lies on another computer.
    pub(super) fn offline_blocks(&self, id: &str) -> bool {
        self.view_offline() && Origin::parse(id).is_ok_and(|origin| origin.scheme != "http")
    }

    fn view_offline(&self) -> bool {
        lock(&self.view).offline
    }
}
