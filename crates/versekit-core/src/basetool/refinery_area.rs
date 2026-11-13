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

//! The area refinery of the sync: the refinery jobs against the own refinery orders of the Basetool. The plan is
//! in [`refinery_sync`](crate::basetool::refinery_sync).
//!
//! **Isolated.** An error here stops no other area; the counts carry its code.
//!
//! **Sparing.** One feed request per pass from the cursor on; the places and the raw materials only while a job
//! waits to become an order (places once an hour, an unknown material once a day); a change set only when there is
//! something to change, with its `Idempotency-Key`, behind the hold and the `Retry-After` of the route, and an
//! operation answered before is not sent again while it stays the same.
//!
//! **Keys before sending.** A job gets its own key written into its file before its order is created: a pass that
//! breaks off after the send finds the order by the same key and does not create a second one.

use std::collections::{BTreeMap, BTreeSet};

use crate::basetool::area_support::{Posted, pull_generic, put_object, send_unless_held, store_busy, take_object, text_map};
use crate::basetool::error::ApiError;
use crate::basetool::feed;
use crate::basetool::orgunits::OrgUnit;
use crate::basetool::refinery_sync::{self, Inputs, Order, ROUTE_READ};
use crate::basetool::scopes::{ROUTE_REFINERY, SETTING_REFINERY, SETTING_REFINERY_UNIT};
use crate::basetool::sync::{Area, Pass, Shared};
use crate::jsonio::{self, Map, Value, map_set};
use crate::paths::Paths;
use crate::refinery_jobs;

/// The part of the sync state of this area.
pub const STATE: &str = "refinery";

/// The area.
pub struct RefineryArea {
    /// The units of the member, for the owner of a new order.
    pub units: Shared<Vec<OrgUnit>>,
}

impl RefineryArea {
    pub fn new(units: Shared<Vec<OrgUnit>>) -> RefineryArea {
        RefineryArea { units }
    }

    /// The unit of new orders: the chosen one while the member is in it, else the only one; only with a fetched
    /// list, so a unit left is never sent.
    pub fn unit(paths: &Paths, units: Option<&[OrgUnit]>) -> Option<String> {
        let units = units?;
        let wanted = paths.setting(SETTING_REFINERY_UNIT).unwrap_or_default();
        let wanted = crate::text::py_strip(&wanted);
        if !wanted.is_empty() {
            return units.iter().find(|u| u.id == wanted).map(|u| u.id.clone());
        }
        match units {
            [only] => Some(only.id.clone()),
            _ => None,
        }
    }

    /// The open orders of the feed by id, the stand of the last pass carried on; closed ones are left out.
    fn pull(conn: &crate::basetool::connection::Connection, sub: &mut Map) -> Result<BTreeMap<String, Order>, ApiError> {
        let (items, _) = pull_generic(conn, sub, ROUTE_READ, "id")?;
        let open: BTreeMap<String, Value> = items
            .into_iter()
            .filter(|(_, item)| refinery_sync::parse_order(item).is_some_and(|o| o.open()))
            .collect();
        map_set(sub, "server", Value::Object(open.iter().map(|(k, v)| (k.clone(), v.clone())).collect()));
        Ok(open.values().filter_map(refinery_sync::parse_order).map(|o| (o.id.clone(), o)).collect())
    }

    /// The raw material of each refined one of the jobs that wait to become an order.
    fn raw_materials(pass: &Pass<'_>, sub: &mut Map, materials: &BTreeSet<String>) -> Result<BTreeMap<String, String>, ApiError> {
        if materials.is_empty() {
            return Ok(BTreeMap::new());
        }
        let refs: Vec<(String, Value)> = materials
            .iter()
            .flat_map(|m| refinery_sync::raw_candidates(m))
            .map(|name| (name.clone(), Value::Object(vec![("name".into(), Value::str(name))])))
            .collect();
        let mut cache = take_object(sub, "resolved");
        let found = feed::resolve_refs(pass.conn, "MATERIAL", &refs, &mut cache, pass.now);
        put_object(sub, "resolved", cache);
        let found: BTreeMap<String, String> = found?.into_iter().collect();
        Ok(materials
            .iter()
            .filter_map(|m| {
                let bt = refinery_sync::raw_candidates(m).iter().find_map(|c| found.get(c).cloned())?;
                Some((m.to_lowercase(), bt))
            })
            .collect())
    }

    fn sync(&self, pass: &mut Pass<'_>, sub: &mut Map) -> Result<Map, ApiError> {
        let mut server = Self::pull(pass.conn, sub)?;
        let jobs = refinery_jobs::load(pass.paths);
        let mut links: BTreeMap<String, String> = text_map(sub, "links");

        // What waits to become an order needs the places and the raw materials; nothing else does.
        let waiting: Vec<&Map> = jobs
            .iter()
            .filter(|job| {
                refinery_jobs::text(job, refinery_jobs::ORDER).is_empty()
                    && !refinery_jobs::picked_up(job)
                    && !links.contains_key(&refinery_jobs::text(job, refinery_jobs::KEY))
                    && !refinery_jobs::goods(job).is_empty()
            })
            .collect();
        let materials: BTreeSet<String> =
            waiting.iter().flat_map(|job| refinery_jobs::goods(job)).map(|g| g.material).collect();
        let raw = Self::raw_materials(pass, sub, &materials)?;
        let places = if waiting.is_empty() { Map::new() } else { pass.locations.get(pass.conn, pass.now)?.clone() };
        let units = self.units.get().1;
        let unit = Self::unit(pass.paths, units.as_deref().map(Vec::as_slice));

        let mut plan = refinery_sync::plan(&Inputs {
            jobs: &jobs,
            server: &server,
            links: &links,
            places: &places,
            raw: &raw,
            unit: unit.as_deref(),
            now: pass.now,
        });
        // The keys go into the file before their orders are created; a job changed meanwhile waits a pass.
        if !plan.new_keys.is_empty() {
            let wanted: Vec<(Map, String)> =
                plan.new_keys.iter().map(|(index, key)| (jobs[*index].clone(), key.clone())).collect();
            let mut written: BTreeSet<String> = BTreeSet::new();
            let saved = pass.apply_local(|| {
                refinery_jobs::update(pass.paths, |now_jobs| {
                    for (job, key) in &wanted {
                        let same = now_jobs.iter_mut().find(|j| {
                            refinery_jobs::text(j, refinery_jobs::KEY).is_empty()
                                && jsonio::py_eq_value(&Value::Object((*j).clone()), &Value::Object(job.clone()))
                        });
                        if let Some(same) = same {
                            map_set(same, refinery_jobs::KEY, Value::str(key));
                            written.insert(key.clone());
                        }
                    }
                    !written.is_empty()
                })
            });
            if !saved {
                return Err(store_busy("refinery jobs not written"));
            }
            let new: BTreeSet<String> = plan.new_keys.iter().map(|(_, key)| key.clone()).collect();
            plan.ops.retain(|(key, _)| !new.contains(key) || written.contains(key));
        }
        let sets = refinery_sync::change_sets(&plan);
        let Posted { failed, applied, posted, paused, waiting: wait, .. } =
            send_unless_held(pass, ROUTE_REFINERY, &sets)?;
        if applied > 0 || failed.iter().any(|(_, _, reason)| reason == "ALREADY_EXISTS") {
            server = Self::pull(pass.conn, sub)?;
        }

        // Links: every open order with the key of an own job; a key the Basetool knows but the feed does not show
        // (closed already) stays linked to nothing, so it is never created again.
        let jobs = refinery_jobs::load(pass.paths);
        let own_keys: BTreeSet<String> = jobs
            .iter()
            .map(|job| refinery_jobs::text(job, refinery_jobs::KEY))
            .filter(|key| !key.is_empty())
            .collect();
        for order in server.values() {
            if let Some(key) = order.client_key.as_ref().filter(|k| own_keys.contains(*k)) {
                links.insert(key.clone(), order.id.clone());
            }
        }
        let posted: BTreeSet<&String> = posted.iter().collect();
        let refused: BTreeMap<String, String> = failed
            .iter()
            .map(|(key, kind, reason)| (key.clone(), if reason.is_empty() { kind.clone() } else { reason.clone() }))
            .collect();
        for (key, op) in &plan.ops {
            let created = op.get("op").and_then(Value::as_str) == Some("create");
            let known = refused.get(key).is_none_or(|reason| reason == "ALREADY_EXISTS");
            if created && posted.contains(key) && known {
                links.entry(key.clone()).or_default();
            }
        }
        links.retain(|key, _| own_keys.contains(key));

        // The orders over there into the own jobs.
        let mut taken = refinery_sync::Taken::default();
        let saved = pass.apply_local(|| {
            refinery_jobs::update(pass.paths, |now_jobs| {
                let keys: BTreeSet<String> = now_jobs
                    .iter()
                    .map(|job| refinery_jobs::text(job, refinery_jobs::KEY))
                    .filter(|key| !key.is_empty())
                    .collect();
                taken = refinery_sync::take_over(now_jobs, &server, &keys, pass.now);
                taken.any()
            })
        });
        if !saved {
            return Err(store_busy("refinery jobs not written"));
        }

        map_set(sub, "links", Value::Object(links.iter().map(|(k, v)| (k.clone(), Value::str(v))).collect()));
        map_set(sub, "rejected", Value::Object(refused.iter().map(|(k, v)| (k.clone(), Value::str(v))).collect()));
        let sent = |kind: &str| {
            plan.ops
                .iter()
                .filter(|(key, op)| {
                    op.get("op").and_then(Value::as_str) == Some(kind) && posted.contains(key) && !refused.contains_key(key)
                })
                .count() as i64
        };
        let mut counts = Map::new();
        for (name, value) in [
            ("orders", server.len() as i64),
            ("created", sent("create")),
            ("updated", sent("update")),
            ("picked_up", sent("pickup")),
            ("taken", taken.added as i64),
            ("dropped", taken.dropped as i64),
            ("rejected", refused.len() as i64),
            ("no_place", plan.no_place as i64),
            ("no_material", plan.no_material as i64),
            ("no_unit", plan.no_unit as i64),
            ("paused", i64::from(paused)),
            ("waiting", i64::from(wait.is_some())),
        ] {
            map_set(&mut counts, name, Value::int(value));
        }
        Ok(counts)
    }
}

/// Whether the file of the jobs can be read: missing is fine (then there are none), an object with the list too.
/// ⚠ A damaged file would read as no jobs, and taking the orders over would write over it.
fn jobs_readable(paths: &Paths) -> bool {
    let path = paths.app_file(refinery_jobs::FILE);
    if !std::path::Path::new(&path).exists() {
        return true;
    }
    matches!(jsonio::read_json(&path), Ok(Value::Object(data)) if matches!(data.iter().find(|(k, _)| k == "auftraege"), Some((_, Value::Array(_)))))
}

impl Area for RefineryArea {
    fn name(&self) -> &'static str {
        "refinery"
    }

    fn runs(&self, ready: &[&'static str], _capabilities: &[String]) -> bool {
        ready.contains(&SETTING_REFINERY)
    }

    fn isolated(&self) -> bool {
        true
    }

    /// Nothing is held in memory: the state stays for the next pass, a wait included.
    fn forget_after(&self, _action: crate::basetool::error::Action) -> bool {
        false
    }

    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError> {
        if !jobs_readable(pass.paths) {
            crate::errors::record("basetool_sync.refinery", &"Raffinerieauftraege unlesbar, nicht abgeglichen");
            return Ok(vec![("unreadable".into(), Value::int(1))]);
        }
        let mut sub = take_object(pass.state, STATE);
        let result = self.sync(pass, &mut sub);
        put_object(pass.state, STATE, sub);
        result
    }

    fn forget(&mut self) {}
}
