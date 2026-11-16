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

//! The work behind the page "Connect org", in threads of its own: the login
//! (discovery, code, waiting for the member), checking a connection and
//! disconnecting. The page reads a [`View`] each frame; it holds no token.
//!
//! One login runs at a time; a new one or "cancel" ends the one before.
//! Checking and disconnecting run one after the other under the lock of the
//! store, so a refresh token is never used twice.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use crate::orgstd::client::{OrgClient, Poll};
use crate::orgstd::store::{Connection, Store};
use crate::orgstd::{E_EXPIRED, E_GONE, E_STORE, OrgError};

pub use data::{Kind, MAX_AGE, OrgData, PAUSE, Slot, SlotInfo};
pub use write::{Booking, FILE as SHARED_FILE, Shared, Thing};

mod data;
mod write;

/// The seconds since 1970.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The clock of the system.
pub fn system_clock() -> Clock {
    Arc::new(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
}

/// What the page shows of one connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionView {
    /// The address of the org, which names the connection.
    pub id: String,
    pub name: String,
    pub tag: String,
    pub member: String,
    pub scopes: Vec<String>,
    pub connected_at: u64,
    pub checked_at: Option<u64>,
}

impl ConnectionView {
    fn of(connection: &Connection) -> ConnectionView {
        // The member's own name and the address of the org are private in streamer mode.
        crate::streamer::learn(&connection.member);
        crate::streamer::learn(&connection.id());
        ConnectionView {
            id: connection.id(),
            name: connection.discovery.name.clone(),
            tag: connection.discovery.tag.clone(),
            member: connection.member.clone(),
            scopes: connection.scopes.clone(),
            connected_at: connection.connected_at,
            checked_at: connection.checked_at,
        }
    }
}

/// Where the login stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Login {
    /// The discovery document and the code are being fetched.
    Looking { address: String },
    /// The member types `user_code` at `verification_uri`.
    Code { org: String, user_code: String, verification_uri: String },
    /// Connected as `member` to `org`.
    Connected { org: String, member: String },
    Failed(OrgError),
}

/// What the last check or disconnecting of a connection gave.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Checked,
    Failed(OrgError),
    /// Disconnected; `false` when the server could not be told and only this computer forgot it.
    Disconnected { told: bool },
}

/// Everything the page draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// Grows with every change.
    pub generation: u64,
    pub connections: Vec<ConnectionView>,
    pub login: Option<Login>,
    /// The connections being checked or disconnected.
    pub busy: Vec<String>,
    /// The last outcome per connection; that of a disconnected one stays until the next login.
    pub outcomes: Vec<(String, Outcome)>,
    /// The stored connections could not be read.
    pub store_failed: bool,
    /// Fetching is switched off (`VERSEKIT_NO_NET`): nothing is asked of an org on another computer.
    pub offline: bool,
}

impl View {
    /// Whether an element that needs the right `scope` of an org shows: some connected org granted it. Every
    /// switch, overlay row, page, sync button, tile, search hit and required field of the open org standard
    /// outside the page "Connect org" asks this; without a connection it is hidden and not found.
    pub fn shows(&self, scope: &str) -> bool {
        self.connections.iter().any(|c| c.scopes.iter().any(|s| s == scope))
    }
}

struct Inner {
    client: OrgClient,
    store: Store,
    /// Held while the store is read, changed and written back.
    store_lock: Mutex<()>,
    view: Mutex<View>,
    clock: Clock,
    /// Grows with every new login and every "cancel": a login whose number is no longer the newest stops.
    login_number: AtomicU64,
    /// The shortest wait between two polls, whatever the server says.
    min_interval: Duration,
    /// What the orgs shared, per org; in memory only.
    data: Mutex<HashMap<String, OrgData>>,
    /// One lock per org, held while its tokens are in use: they change with every refresh.
    net_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// What was shared and booked, per org.
    writes: Mutex<write::Writes>,
}

/// The work behind the page; cheap to clone.
#[derive(Clone)]
pub struct Hub {
    inner: Arc<Inner>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Hub {
    /// A hub over `store`; reads the stored connections at once.
    pub fn new(client: OrgClient, store: Store, clock: Clock) -> Hub {
        Hub::with_min_interval(client, store, clock, Duration::from_secs(1))
    }

    /// [`Hub::new`] with another shortest wait between polls (tests).
    pub fn with_min_interval(client: OrgClient, store: Store, clock: Clock, min_interval: Duration) -> Hub {
        Hub::build(client, store, clock, min_interval, false)
    }

    /// [`Hub::new`] for a computer where fetching is switched off (`VERSEKIT_NO_NET`) when `offline` is set: no
    /// request goes to an org on another computer.
    pub fn with_offline(client: OrgClient, store: Store, clock: Clock, offline: bool) -> Hub {
        Hub::build(client, store, clock, Duration::from_secs(1), offline)
    }

    fn build(client: OrgClient, store: Store, clock: Clock, min_interval: Duration, offline: bool) -> Hub {
        let hub = Hub {
            inner: Arc::new(Inner {
                client,
                store,
                store_lock: Mutex::new(()),
                view: Mutex::new(View { offline, ..View::default() }),
                clock,
                login_number: AtomicU64::new(0),
                min_interval,
                data: Mutex::new(HashMap::new()),
                net_locks: Mutex::new(HashMap::new()),
                writes: Mutex::new(write::Writes::default()),
            }),
        };
        hub.load_shared();
        hub.reload();
        hub
    }

    /// What the page draws now.
    pub fn view(&self) -> View {
        lock(&self.inner.view).clone()
    }

    /// See [`View::shows`].
    pub fn shows(&self, scope: &str) -> bool {
        lock(&self.inner.view).shows(scope)
    }

    /// Whether anything runs in the background.
    pub fn working(&self) -> bool {
        let view = lock(&self.inner.view);
        !view.busy.is_empty()
            || matches!(view.login, Some(Login::Looking { .. } | Login::Code { .. }))
            || lock(&self.inner.data).values().any(OrgData::loading)
    }

    fn change(&self, edit: impl FnOnce(&mut View)) {
        let mut view = lock(&self.inner.view);
        edit(&mut view);
        view.generation += 1;
    }

    fn reload(&self) {
        let _store = lock(&self.inner.store_lock);
        let loaded = self.inner.store.load();
        self.change(|view| {
            view.store_failed = loaded.is_err();
            view.connections = loaded.unwrap_or_default().iter().map(ConnectionView::of).collect();
        });
        self.prune_data();
        self.prune_writes();
    }

    fn set_outcome(&self, id: &str, outcome: Outcome) {
        self.change(|view| {
            view.busy.retain(|b| b != id);
            view.outcomes.retain(|(known, _)| known != id);
            view.outcomes.push((id.to_string(), outcome));
        });
    }

    /// Starts a login at the address the member typed; a login running before ends.
    pub fn connect(&self, typed: &str) {
        let number = self.inner.login_number.fetch_add(1, Ordering::SeqCst) + 1;
        let address = typed.trim().to_string();
        self.change(|view| view.login = Some(Login::Looking { address: address.clone() }));
        let hub = self.clone();
        std::thread::Builder::new()
            .name("orga-anmeldung".into())
            .spawn(move || {
                let result = hub.run_login(number, &address);
                if hub.current(number) {
                    hub.change(|view| {
                        view.login = Some(match result {
                            Ok((org, member)) => Login::Connected { org, member },
                            Err(error) => Login::Failed(error),
                        })
                    });
                }
            })
            .expect("login thread");
    }

    /// Ends the login that runs; the page shows no login afterwards.
    pub fn cancel_login(&self) {
        self.inner.login_number.fetch_add(1, Ordering::SeqCst);
        self.change(|view| view.login = None);
    }

    /// Forgets how the last login ended.
    pub fn clear_login(&self) {
        if !self.working() {
            self.change(|view| view.login = None);
        }
    }

    fn current(&self, number: u64) -> bool {
        self.inner.login_number.load(Ordering::SeqCst) == number
    }

    /// Waits `time` in small steps; `false` when the login was ended meanwhile.
    fn wait(&self, number: u64, time: Duration) -> bool {
        let step = Duration::from_millis(50);
        let mut waited = Duration::ZERO;
        while waited < time {
            if !self.current(number) {
                return false;
            }
            std::thread::sleep(step);
            waited += step;
        }
        self.current(number)
    }

    fn run_login(&self, number: u64, typed: &str) -> Result<(String, String), OrgError> {
        let client = &self.inner.client;
        let discovery = client.discover(typed)?;
        let code = client.start_login(&discovery)?;
        let started = (self.inner.clock)();
        if !self.current(number) {
            return Err(OrgError::new(E_GONE));
        }
        let org = discovery.name.clone();
        self.change(|view| {
            view.login = Some(Login::Code {
                org: org.clone(),
                user_code: code.user_code.clone(),
                verification_uri: code.verification_uri.clone(),
            })
        });
        let mut interval = Duration::from_secs(code.interval).max(self.inner.min_interval);
        let tokens = loop {
            if !self.wait(number, interval) {
                return Err(OrgError::new(E_GONE));
            }
            if (self.inner.clock)() >= started + code.expires_in {
                return Err(OrgError::new(E_EXPIRED));
            }
            match client.poll_login(&discovery, &code)? {
                Poll::Pending => {}
                Poll::SlowDown => interval += Duration::from_secs(5),
                Poll::Done(tokens) => break tokens,
            }
        };
        let now = (self.inner.clock)();
        let mut connection = Connection {
            discovery,
            member: String::new(),
            scopes: Vec::new(),
            access_token: String::new(),
            access_expires: 0,
            refresh_token: String::new(),
            connected_at: now,
            checked_at: None,
        };
        OrgClient::take_tokens(&mut connection, tokens, now);
        // A poll on its way when the login was ended may still bring tokens: they are given back at once.
        if !self.current(number) {
            let _ = client.revoke(&connection.discovery, &connection.refresh_token);
            return Err(OrgError::new(E_GONE));
        }
        // Without an answer of `/me` the connection is stored all the same, without the member's name.
        if let Ok(me) = client.me(&mut connection, now) {
            connection.member = me.member;
            connection.scopes = me.scopes;
            connection.checked_at = Some(now);
        }
        if !self.current(number) {
            let _ = client.revoke(&connection.discovery, &connection.refresh_token);
            return Err(OrgError::new(E_GONE));
        }
        let id = connection.id();
        let member = connection.member.clone();
        {
            let _store = lock(&self.inner.store_lock);
            let mut list = self.inner.store.load().unwrap_or_default();
            let replaced: Vec<Connection> = list.iter().filter(|c| c.id() == id).cloned().collect();
            list.retain(|c| c.id() != id);
            list.push(connection);
            self.inner.store.save(&list).map_err(|_| OrgError::new(E_STORE))?;
            for old in replaced {
                let _ = client.revoke(&old.discovery, &old.refresh_token);
            }
        }
        self.reload();
        self.change(|view| view.outcomes.retain(|(known, _)| known != &id));
        Ok((org, member))
    }

    /// Asks the org server who the member is now; refreshes the token on the way.
    pub fn check(&self, id: &str) {
        self.background(id, "orga-pruefen", |hub, id| {
            let net = hub.net_lock(id);
            let _tokens = lock(&net);
            let _store = lock(&hub.inner.store_lock);
            let mut list = hub.inner.store.load().map_err(|_| OrgError::new(E_STORE))?;
            let Some(connection) = list.iter_mut().find(|c| c.id() == id) else { return Err(OrgError::new(E_GONE)) };
            let now = (hub.inner.clock)();
            let result = hub.inner.client.me(connection, now);
            if let Ok(me) = &result {
                connection.member = me.member.clone();
                connection.scopes = me.scopes.clone();
                connection.checked_at = Some(now);
            }
            hub.inner.store.save(&list).map_err(|_| OrgError::new(E_STORE))?;
            result.map(|_| Outcome::Checked)
        });
    }

    /// Tells the org server to forget the connection and forgets it here, also when the server cannot be reached.
    pub fn disconnect(&self, id: &str) {
        self.background(id, "orga-trennen", |hub, id| {
            let _store = lock(&hub.inner.store_lock);
            let mut list = hub.inner.store.load().map_err(|_| OrgError::new(E_STORE))?;
            let Some(connection) = list.iter().find(|c| c.id() == id).cloned() else {
                return Err(OrgError::new(E_GONE));
            };
            let told = hub.inner.client.revoke(&connection.discovery, &connection.refresh_token).is_ok();
            list.retain(|c| c.id() != id);
            hub.inner.store.save(&list).map_err(|_| OrgError::new(E_STORE))?;
            Ok(Outcome::Disconnected { told })
        });
    }

    fn background(&self, id: &str, name: &str, work: fn(&Hub, &str) -> Result<Outcome, OrgError>) {
        {
            let mut view = lock(&self.inner.view);
            if view.busy.iter().any(|b| b == id) {
                return;
            }
            view.busy.push(id.to_string());
            view.generation += 1;
        }
        let (hub, id) = (self.clone(), id.to_string());
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || {
                let outcome = work(&hub, &id).unwrap_or_else(Outcome::Failed);
                hub.reload();
                hub.set_outcome(&id, outcome);
            })
            .expect("org thread");
    }
}
