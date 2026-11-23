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

//! The stock of the unit at the Basetool as the place a stock scan books into: the amount of the item at the place
//! in the pool of a unit the player is a logistician of (all lots, whoever booked them) is brought to the scanned
//! amount by booking the difference (in for more, out for less; what is set aside for orders only after a
//! confirmation).
//!
//! [`plan`] works out the booking from a snapshot of the pool and sends nothing; [`OrgLink`] fetches the snapshot,
//! looks the item up where the pool does not know it, and sends the change set.

use std::sync::Arc;

use super::{Measure, ScannedItem, UnitError, UnitStock};
use crate::basetool::connection::{Clock, Connection, system_clock};
use crate::basetool::orgstock::{
    self, Entry, Lot, OrgStock, OrgStockData, PIECE, Resolved, SCU, op_book_in, op_book_out,
};
use crate::basetool::scopes::{SCOPES_ORGSTOCK_WRITE, SETTING_ORGSTOCK_WRITE};
use crate::jsonio::Value;
use crate::paths::Paths;
use crate::stock_data::lower;
use crate::text::py_strip;

/// The setting that names the unit a scan books into, where the player may write in several (its id); the first
/// unit he may write in is taken without it.
pub const SETTING_UNIT: &str = "lager_scan_einheit";

/// The note a booking of a scan carries.
const NOTE: &str = "VerseKit";

/// What one scan does to the pool: the amount the player's entry held before and the operations to send.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub before: Option<f64>,
    /// Empty where the amount is the wanted one already.
    pub ops: Vec<Value>,
}

fn unit_word(measure: Measure) -> &'static str {
    match measure {
        Measure::Pieces => PIECE,
        Measure::Scu => SCU,
    }
}

fn same_lot(lot: &Lot, unit_id: &str, item: &ScannedItem, place: &str) -> bool {
    let name = lower(py_strip(&item.name));
    let quality = if item.measure == Measure::Pieces { 0 } else { i64::from(item.quality.unwrap_or(0)) };
    lot.unit_id == unit_id
        && !lot.stolen
        && lot.unit_q == unit_word(item.measure)
        && lot.quality == quality
        && lower(py_strip(&lot.name)) == name
        && lower(py_strip(&lot.place)) == lower(py_strip(place))
}

/// How much of the item lies at the place in the pool, whoever booked it (the sum of all its lots), and the lots.
fn lots_at<'a>(data: &'a OrgStockData, unit_id: &str, item: &ScannedItem, place: &str) -> (Vec<&'a Lot>, f64) {
    let lots: Vec<&Lot> = data.lots.iter().filter(|lot| same_lot(lot, unit_id, item, place)).collect();
    let total = lots.iter().map(|lot| lot.amount).sum();
    (lots, total)
}

/// What the entries hold that orders and missions have set aside: the amount of the entries minus what is free.
fn reserved_in(entries: &[&Entry]) -> f64 {
    entries.iter().map(|e| (e.amount - e.free()).max(0.0)).sum()
}

/// The earmarks to take `extra` out of the reservations of `entry`, jobs first and then missions, each up to `extra`.
fn earmarks_for(entry: &Entry, extra: f64) -> (orgstock::Earmarks, orgstock::Earmarks) {
    let mut jobs = orgstock::Earmarks::new();
    let mut left = extra;
    for line in entry.jobs.iter().filter(|j| !j.delivered) {
        let take = left.min(line.amount);
        if take > 0.0 {
            jobs.insert(line.id.clone(), take);
            left -= take;
        }
    }
    let mut missions = orgstock::Earmarks::new();
    let mut left = extra;
    for line in &entry.missions {
        let take = left.min(line.amount);
        if take > 0.0 {
            missions.insert(line.id.clone(), take);
            left -= take;
        }
    }
    (jobs, missions)
}

/// The booking that brings the amount of the item at `place` in the pool to `amount` (`None` takes all out), from
/// the snapshot `data`.
///
/// The amount before is the sum of all lots of the item at the place, whoever booked them: the scan counts the real
/// stock. Only the difference is booked: more is booked in for the player (`own_ref`, into his own entry or a new
/// lot), less is booked out of the entries that are there, his own first. What is set aside for orders and missions
/// is never taken silently: without `force` that is [`UnitError::Reserved`] with the amount set aside, with `force`
/// it is taken out of those reservations. `material` is the id of the item at the Basetool where the pool does not
/// know it yet.
#[allow(clippy::too_many_arguments)]
pub fn plan(
    data: &OrgStockData,
    unit_id: &str,
    own_ref: &str,
    item: &ScannedItem,
    place: &str,
    amount: Option<f64>,
    material: Option<&str>,
    force: bool,
) -> Result<Plan, UnitError> {
    if !data.can_write(unit_id) {
        return Err(UnitError::NoRight);
    }
    let (lots, total) = lots_at(data, unit_id, item, place);
    let before = (!lots.is_empty()).then_some(total);
    let measure = unit_word(item.measure);
    let delta = crate::stock::py_round(amount.unwrap_or(0.0) - total, 3);
    let smallest = if item.measure == Measure::Pieces { 0.5 } else { 0.0005 };
    if delta.abs() < smallest {
        return Ok(Plan { before, ops: Vec::new() });
    }
    if delta > 0.0 {
        if own_ref.is_empty() {
            return Err(UnitError::NoRight);
        }
        let bt = match lots.iter().map(|l| l.material_bt.clone()).find(|bt| !bt.is_empty()) {
            Some(bt) => bt,
            None => match data.known_material(&item.name, measure) {
                Some(found) => found.bt,
                None => material.map(str::to_string).ok_or_else(|| UnitError::Unknown(item.name.clone()))?,
            },
        };
        let op = op_book_in(
            unit_id,
            own_ref,
            Value::Object(vec![("bt".into(), Value::str(bt))]),
            Value::Object(vec![("name".into(), Value::str(py_strip(place)))]),
            if item.measure == Measure::Pieces { 0 } else { i64::from(item.quality.unwrap_or(0)) },
            false,
            delta,
            measure,
            NOTE,
            true,
        );
        return Ok(Plan { before, ops: vec![op] });
    }
    // Less: out of the entries, the player's own first. Without the entries of a lot the amounts are not known.
    let mut entries: Vec<&Entry> = Vec::new();
    for lot in &lots {
        entries.extend(lot.entries.as_ref().ok_or(UnitError::NoRight)?.iter());
    }
    entries.sort_by_key(|e| e.holder_ref != own_ref);
    let mut left = -delta;
    let mut takes: Vec<(&Entry, f64, f64)> = Vec::new();
    for entry in &entries {
        let free = left.min(entry.free().min(entry.amount).max(0.0));
        if free > 0.0 {
            takes.push((entry, free, 0.0));
            left -= free;
        }
    }
    if left > smallest / 2.0 {
        if !force {
            return Err(UnitError::Reserved(reserved_in(&entries)));
        }
        for entry in &entries {
            let taken = takes.iter().find(|t| std::ptr::eq(t.0, *entry)).map_or(0.0, |t| t.1);
            let extra = left.min((entry.amount - taken).max(0.0));
            if extra <= 0.0 {
                continue;
            }
            match takes.iter_mut().find(|t| std::ptr::eq(t.0, *entry)) {
                Some(slot) => slot.2 = extra,
                None => takes.push((entry, 0.0, extra)),
            }
            left -= extra;
        }
        if left > smallest / 2.0 {
            return Err(UnitError::Failed("MORE_THAN_HELD".into()));
        }
    }
    let ops = takes
        .into_iter()
        .map(|(entry, free, extra)| {
            let (jobs, missions) = if extra > 0.0 { earmarks_for(entry, extra) } else { Default::default() };
            op_book_out(unit_id, entry, free + extra, measure, &jobs, &missions)
        })
        .collect();
    Ok(Plan { before, ops })
}

/// The connection of this program to the Basetool with what a booking needs of it.
pub struct OrgLink {
    conn: Arc<Connection>,
    store: Arc<OrgStock>,
    own_account: Option<String>,
    /// Read at every booking: the unit to book into ([`SETTING_UNIT`]) and the switch that lets the program write at
    /// the Basetool at all.
    paths: Paths,
    clock: Clock,
}

impl OrgLink {
    /// A link over `conn`, for the account `own_account`; the settings of `paths` decide where it may write.
    pub fn new(conn: Arc<Connection>, own_account: Option<String>, paths: &Paths) -> OrgLink {
        OrgLink { conn, store: OrgStock::new(), own_account, paths: paths.working_copy(), clock: system_clock() }
    }

    /// The link of the settings, with the credentials the system keeps for the main window. `None` where no
    /// connection can be made (the reason is in the error log).
    pub fn system(paths: &Paths, own_account: Option<String>, version: &str) -> Option<OrgLink> {
        use crate::basetool::http::HttpTransport;
        use crate::basetool::key::system_keys;
        use crate::basetool::secrets::SystemSecrets;
        let host = paths.host.clone();
        let config = crate::basetool::config::Config::from_host(&host);
        let transport = match HttpTransport::new(&config.ca, &host) {
            Ok(transport) => Arc::new(transport),
            Err(error) => {
                crate::errors::record("stock_scan_book.transport", &error);
                return None;
            }
        };
        let secrets = Arc::new(SystemSecrets::from_host(&host));
        let keys = system_keys(secrets.clone(), &host);
        let budget = crate::testmode::basetool_write_ceiling(paths).map(crate::basetool::guard::run_budget);
        let conn = Arc::new(
            Connection::new(config, transport, secrets, keys, version, system_clock())
                .with_test_lock(crate::testmode::basetool_locked(paths))
                .with_kept_key(crate::testmode::basetool_keeps_key(paths))
                .with_write_budget(budget)
                .with_journal(crate::basetool::guard::Journal::from_env(paths)),
        );
        Some(OrgLink::new(conn, own_account, paths))
    }

    fn may_write(&self) -> bool {
        let granted = self.conn.granted();
        self.paths.setting_bool(SETTING_ORGSTOCK_WRITE, false)
            && self.conn.connected()
            && SCOPES_ORGSTOCK_WRITE.iter().all(|s| granted.iter().any(|g| g == s))
    }

    fn fetch(&self) -> Result<Arc<OrgStockData>, UnitError> {
        let now = (self.clock)();
        self.store.refresh(&self.conn, true, true, now).map_err(|e| UnitError::Failed(e.code))?;
        self.store.current(now).1.ok_or_else(|| UnitError::Failed("NO_DATA".into()))
    }

    fn unit_of(&self, data: &OrgStockData) -> Option<String> {
        let writable = data.writable_units();
        let wanted = self.paths.setting(SETTING_UNIT);
        let chosen = wanted.as_ref().and_then(|id| writable.iter().find(|u| &u.id == id));
        chosen.or_else(|| writable.first()).map(|u| u.id.clone())
    }
}

impl UnitStock for OrgLink {
    fn set(
        &self,
        item: &ScannedItem,
        place: &str,
        amount: Option<f64>,
        force: bool,
    ) -> Result<Option<f64>, UnitError> {
        if !self.may_write() {
            return Err(UnitError::NoRight);
        }
        let data = self.fetch()?;
        let unit_id = self.unit_of(&data).ok_or(UnitError::NoRight)?;
        let own_ref = data
            .rosters
            .get(&unit_id)
            .map(|roster| orgstock::own_member(roster, self.own_account.as_deref()))
            .unwrap_or_default();
        let planned = match plan(&data, &unit_id, &own_ref, item, place, amount, None, force) {
            Err(UnitError::Unknown(name)) => {
                let piece = item.measure == Measure::Pieces;
                let found = orgstock::resolve_material(&self.conn, &name, None, piece)
                    .map_err(|e| UnitError::Failed(e.code))?;
                let bt = match found {
                    Resolved::One(found) => found.bt,
                    Resolved::Several(_) | Resolved::Unmatched => return Err(UnitError::Unknown(name)),
                };
                plan(&data, &unit_id, &own_ref, item, place, amount, Some(&bt), force)?
            }
            other => other?,
        };
        if planned.ops.is_empty() {
            return Ok(planned.before);
        }
        let outcome = orgstock::send(&self.conn, planned.ops).map_err(|e| UnitError::Failed(e.code))?;
        if outcome.held {
            return Err(UnitError::Refused(vec!["HELD".into()]));
        }
        if !outcome.failed.is_empty() {
            return Err(UnitError::Refused(outcome.reasons()));
        }
        Ok(planned.before)
    }
}
