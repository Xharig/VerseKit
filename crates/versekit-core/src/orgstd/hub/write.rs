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

//! What goes out to an org: the own stock, the own blueprints and the
//! delivery to an order. Each only on a call of the page after the member
//! confirmed it, one request each, never in a loop.
//!
//! The standard replaces the whole list, so a list that is empty or too long
//! never goes out through [`Hub::share_stock`] and [`Hub::share_blueprints`]:
//! ending the sharing is a call of its own ([`Hub::end_share`]) that sends the
//! empty list and nothing else does. A booking carries a key made when its
//! form opened; sending it again with the same key books once.
//!
//! The time and the number of the last sharing are kept in `teilen.json` next
//! to the connections: no names, no quantities.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::orgstd::data::{OrgOrder, parse_orders};
use crate::orgstd::share::{BlueprintShare, StockShare};
use crate::orgstd::{E_ANSWER, E_OFFLINE, OrgError};

use super::{Hub, lock};

/// The file next to the connections with the last sharings.
pub const FILE: &str = "teilen.json";

/// What can be shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Thing {
    Stock,
    Blueprints,
}

impl Thing {
    fn scope(self) -> &'static str {
        match self {
            Thing::Stock => "stock:write",
            Thing::Blueprints => "blueprints:write",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Thing::Stock => "bestand",
            Thing::Blueprints => "bauplaene",
        }
    }
}

/// How the sharing of one thing with one org stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shared {
    /// The last sharing that went through: when and how many lines were sent; `None` before the first.
    pub at: Option<u64>,
    pub sent: usize,
    /// How many the org says it stored, when it says.
    pub stored: Option<u64>,
    /// The last sharing ended it: the org holds nothing of the member.
    pub ended: bool,
    /// Why the last try failed.
    pub error: Option<OrgError>,
    /// A request runs.
    pub running: bool,
}

/// How a booking stands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Booking {
    pub running: bool,
    /// The order the last booking was for.
    pub order: String,
    /// `Ok` with the order as it stands now, or why it was not booked.
    pub result: Option<Result<OrgOrder, OrgError>>,
}

/// The state of everything that goes out.
#[derive(Default)]
pub(super) struct Writes {
    shared: HashMap<(String, Thing), Shared>,
    bookings: HashMap<String, Booking>,
}

fn file_of(hub: &Hub) -> Option<std::path::PathBuf> {
    hub.inner.store.file().and_then(|f| std::path::Path::new(f).parent().map(|p| p.join(FILE)))
}

impl Hub {
    /// Reads the last sharings from their file.
    pub(super) fn load_shared(&self) {
        let Some(file) = file_of(self) else { return };
        let Ok(bytes) = std::fs::read(file) else { return };
        let Ok(root) = serde_json::from_slice::<Value>(&bytes) else { return };
        let mut writes = lock(&self.inner.writes);
        for (id, entry) in root.as_object().into_iter().flatten() {
            for thing in [Thing::Stock, Thing::Blueprints] {
                let Some(one) = entry.get(thing.key()) else { continue };
                let at = one.get("zeit").and_then(Value::as_u64);
                let sent = one.get("zeilen").and_then(Value::as_u64).unwrap_or(0) as usize;
                let ended = one.get("beendet").and_then(Value::as_bool).unwrap_or(false);
                writes.shared.insert((id.clone(), thing), Shared { at, sent, ended, ..Shared::default() });
            }
        }
    }

    fn save_shared(&self) {
        let Some(file) = file_of(self) else { return };
        let writes = lock(&self.inner.writes);
        let mut root = serde_json::Map::new();
        for ((id, thing), shared) in &writes.shared {
            let Some(at) = shared.at else { continue };
            let entry = root.entry(id.clone()).or_insert_with(|| json!({}));
            entry[thing.key()] = json!({ "zeit": at, "zeilen": shared.sent, "beendet": shared.ended });
        }
        let temp = file.with_extension("tmp");
        let text = serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default();
        if std::fs::write(&temp, text).is_ok() && std::fs::rename(&temp, &file).is_err() {
            let _ = std::fs::remove_file(&temp);
        }
    }

    /// Forgets what was shared with orgs that are gone.
    pub(super) fn prune_writes(&self) {
        let ids: Vec<String> = lock(&self.inner.view).connections.iter().map(|c| c.id.clone()).collect();
        let changed = {
            let mut writes = lock(&self.inner.writes);
            let before = writes.shared.len();
            writes.shared.retain(|(id, _), _| ids.contains(id));
            writes.bookings.retain(|id, _| ids.contains(id));
            writes.shared.len() != before
        };
        if changed {
            self.save_shared();
        }
    }

    /// How the sharing of `thing` with `id` stands.
    pub fn shared(&self, id: &str, thing: Thing) -> Shared {
        lock(&self.inner.writes).shared.get(&(id.to_string(), thing)).copied().unwrap_or_default()
    }

    /// How the last booking at `id` stands.
    pub fn booking(&self, id: &str) -> Booking {
        lock(&self.inner.writes).bookings.get(id).cloned().unwrap_or_default()
    }

    fn may_write(&self, id: &str, scope: &str) -> bool {
        lock(&self.inner.view).connections.iter().any(|c| c.id == id && c.scopes.iter().any(|s| s == scope))
    }

    fn set_shared(&self, id: &str, thing: Thing, edit: impl FnOnce(&mut Shared)) {
        edit(lock(&self.inner.writes).shared.entry((id.to_string(), thing)).or_default());
        self.change(|_| {});
    }

    /// Sends `lines` as the whole list of `thing`, in a thread; returns whether it started. Never starts without
    /// the right, while a request of the same thing runs, or with the net switched off.
    fn start_share(&self, id: &str, thing: Thing, lines: Vec<Value>, ending: bool) -> bool {
        if !self.may_write(id, thing.scope()) || self.shared(id, thing).running {
            return false;
        }
        let now = (self.inner.clock)();
        if self.inner.offline_blocks(id) {
            self.set_shared(id, thing, |s| s.error = Some(OrgError::new(E_OFFLINE)));
            return false;
        }
        self.set_shared(id, thing, |s| {
            s.running = true;
            s.error = None;
        });
        let (hub, id) = (self.clone(), id.to_string());
        let sent = lines.len();
        std::thread::Builder::new()
            .name("orga-teilen".into())
            .spawn(move || {
                let result = hub.call_org(&id, |client, connection, now| match thing {
                    Thing::Stock => client.share_stock(connection, &lines, now),
                    Thing::Blueprints => {
                        let names: Vec<String> = lines.iter().filter_map(|n| n.as_str().map(str::to_string)).collect();
                        client.share_blueprints(connection, &names, now)
                    }
                });
                let stamp = (hub.inner.clock)().max(now);
                hub.set_shared(&id, thing, |s| {
                    s.running = false;
                    match &result {
                        Ok(answer) => {
                            s.at = Some(stamp);
                            s.sent = sent;
                            s.ended = ending;
                            s.stored = answer
                                .get("items_stored")
                                .or_else(|| answer.get("blueprints_stored"))
                                .and_then(Value::as_u64);
                            s.error = None;
                        }
                        Err(error) => s.error = Some(*error),
                    }
                });
                if result.is_ok() {
                    hub.save_shared();
                }
            })
            .expect("org share thread");
        true
    }

    /// Replaces the stock the member shares with `share`. Refused (returns `false`, sends nothing) when the list is
    /// empty or too long: ending the sharing is [`Hub::end_share`].
    pub fn share_stock(&self, id: &str, share: &StockShare) -> bool {
        share.refusal().is_none() && self.start_share(id, Thing::Stock, share.to_json(), false)
    }

    /// Replaces the blueprints the member shares with `share`; refused as [`Hub::share_stock`].
    pub fn share_blueprints(&self, id: &str, share: &BlueprintShare) -> bool {
        share.refusal().is_none()
            && self.start_share(id, Thing::Blueprints, share.names.iter().map(|n| json!(n)).collect(), false)
    }

    /// Ends the sharing of `thing`: the org is sent the empty list and deletes what it has of the member. The only
    /// way an empty list goes out.
    pub fn end_share(&self, id: &str, thing: Thing) -> bool {
        self.start_share(id, thing, Vec::new(), true)
    }

    /// Books a delivery to the order `order` once: `key` was made when the form opened, so a try again after an
    /// error carries the same key and the org books it once. Returns whether a request started.
    pub fn deliver(&self, id: &str, order: &str, items: Vec<(String, f64)>, key: &str) -> bool {
        if items.is_empty() || !self.may_write(id, "orders:write") {
            return false;
        }
        {
            let mut writes = lock(&self.inner.writes);
            let booking = writes.bookings.entry(id.to_string()).or_default();
            if booking.running {
                return false;
            }
            if self.inner.offline_blocks(id) {
                *booking =
                    Booking { running: false, order: order.to_string(), result: Some(Err(OrgError::new(E_OFFLINE))) };
                drop(writes);
                self.change(|_| {});
                return false;
            }
            *booking = Booking { running: true, order: order.to_string(), result: None };
        }
        self.change(|_| {});
        let (hub, id, order, key) = (self.clone(), id.to_string(), order.to_string(), key.to_string());
        std::thread::Builder::new()
            .name("orga-buchen".into())
            .spawn(move || {
                let result = hub
                    .call_org(&id, |client, connection, now| client.deliver(connection, &order, &items, &key, now))
                    .and_then(|answer| {
                        let parsed = parse_orders(std::slice::from_ref(answer.get("order").unwrap_or(&Value::Null)));
                        parsed.rows.into_iter().next().ok_or(OrgError::new(E_ANSWER))
                    });
                if let Ok(updated) = &result {
                    hub.replace_order(&id, updated.clone());
                }
                lock(&hub.inner.writes)
                    .bookings
                    .insert(id.clone(), Booking { running: false, order: order.clone(), result: Some(result) });
                hub.change(|_| {});
            })
            .expect("org booking thread");
        true
    }

    /// Takes `order` into the list of orders in place of the one with its id.
    fn replace_order(&self, id: &str, order: OrgOrder) {
        let mut data = lock(&self.inner.data);
        if let Some(entry) = data.get_mut(id) {
            let rows = std::sync::Arc::make_mut(&mut entry.orders.rows);
            match rows.iter_mut().find(|o| o.id == order.id) {
                Some(slot) => *slot = order,
                None => rows.push(order),
            }
        }
    }
}
