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

//! The switches per org and area (stock, blueprints, orders): off from the
//! start, kept in the setting `orga_bereiche` as
//! `{ "<address>": { "lager": true, "bauplaene": false, "auftraege": false } }`.
//!
//! An area of an org shows only when the switch is on **and** the member
//! granted the right **and** the connection stands. Without an entry
//! everything is off; disconnecting removes the entry of the org.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use crate::jsonio::{Map, Value, map_set};
use crate::orgstd::hub::{ConnectionView, Kind, View};
use crate::paths::Paths;

/// The setting that holds the switches.
pub const SETTING: &str = "orga_bereiche";

impl Kind {
    /// The name of the area in the setting.
    pub fn key(self) -> &'static str {
        match self {
            Kind::Stock => "lager",
            Kind::Blueprints => "bauplaene",
            Kind::Orders => "auftraege",
        }
    }
}

/// Where the switches are kept.
trait Store: Send + Sync {
    fn on(&self, org: &str, kind: Kind) -> bool;
    fn set(&self, org: &str, kind: Kind, on: bool) -> bool;
    fn forget(&self, org: &str) -> bool;
}

/// In the settings file of the program.
struct InSettings(Paths);

fn object(value: Option<Value>) -> Map {
    match value {
        Some(Value::Object(map)) => map,
        _ => Vec::new(),
    }
}

impl Store for InSettings {
    fn on(&self, org: &str, kind: Kind) -> bool {
        let all = object(self.0.settings_value(SETTING));
        let Some((_, Value::Object(of_org))) = all.iter().find(|(address, _)| address == org) else { return false };
        matches!(of_org.iter().find(|(name, _)| name == kind.key()), Some((_, Value::Bool(true))))
    }

    fn set(&self, org: &str, kind: Kind, on: bool) -> bool {
        let mut all = object(self.0.settings_value(SETTING));
        let mut of_org = object(all.iter().find(|(address, _)| address == org).map(|(_, v)| v.clone()));
        map_set(&mut of_org, kind.key(), Value::Bool(on));
        map_set(&mut all, org, Value::Object(of_org));
        self.0.set_setting(SETTING, Value::Object(all))
    }

    fn forget(&self, org: &str) -> bool {
        let mut all = object(self.0.settings_value(SETTING));
        if !all.iter().any(|(address, _)| address == org) {
            return true;
        }
        all.retain(|(address, _)| address != org);
        self.0.set_setting(SETTING, Value::Object(all))
    }
}

/// In memory only: the preview and tests.
#[derive(Default)]
struct InMemory(Mutex<BTreeMap<(String, &'static str), bool>>);

impl Store for InMemory {
    fn on(&self, org: &str, kind: Kind) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(org.to_string(), kind.key()))
            .copied()
            .unwrap_or(false)
    }

    fn set(&self, org: &str, kind: Kind, on: bool) -> bool {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).insert((org.to_string(), kind.key()), on);
        true
    }

    fn forget(&self, org: &str) -> bool {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).retain(|(address, _), _| address != org);
        true
    }
}

/// The switches; cheap to clone.
#[derive(Clone)]
pub struct Areas(Arc<dyn Store>);

impl Areas {
    /// Kept in the settings of `paths`.
    pub fn in_settings(paths: Paths) -> Areas {
        Areas(Arc::new(InSettings(paths)))
    }

    /// Kept in memory only.
    pub fn in_memory() -> Areas {
        Areas(Arc::new(InMemory::default()))
    }

    /// Whether the switch of `kind` is on for the org `org`; off without an entry.
    pub fn on(&self, org: &str, kind: Kind) -> bool {
        self.0.on(org, kind)
    }

    /// Turns the switch on or off; `false` when it could not be kept.
    pub fn set(&self, org: &str, kind: Kind, on: bool) -> bool {
        self.0.set(org, kind, on)
    }

    /// Removes the entry of an org that is disconnected.
    pub fn forget(&self, org: &str) -> bool {
        self.0.forget(org)
    }
}

impl std::fmt::Debug for Areas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Areas").finish_non_exhaustive()
    }
}

impl ConnectionView {
    /// Whether the member granted the right of `kind` to this org.
    pub fn granted(&self, kind: Kind) -> bool {
        self.scopes.iter().any(|s| s == kind.scope())
    }
}

impl View {
    /// The connected orgs whose area `kind` shows: switch on, right granted, connection standing.
    pub fn orgs_with<'a>(&'a self, areas: &'a Areas, kind: Kind) -> impl Iterator<Item = &'a ConnectionView> {
        self.connections.iter().filter(move |c| c.granted(kind) && areas.on(&c.id, kind))
    }

    /// Whether the area `kind` shows for `org`, or for any org when `org` is `None`.
    pub fn area_visible(&self, areas: &Areas, org: Option<&str>, kind: Kind) -> bool {
        self.orgs_with(areas, kind).any(|c| org.is_none_or(|id| c.id == id))
    }
}

#[cfg(test)]
mod tests;
