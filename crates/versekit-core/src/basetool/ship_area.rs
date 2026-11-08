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

//! The area hangar of the sync (`basetool_sync._sync_ships`,
//! `_apply_ships`, `_own_ship_names`, `_take_ship_names`): the ships of the
//! hangar against the ships of the Basetool. The plan and the change sets are
//! in [`ship_sync`](crate::basetool::ship_sync).
//!
//! The hangar is written here after taking over (inside
//! [`Pass::apply_local`]), the entries looked for by their identifier, never by
//! their position. The own names that were changed over there go into
//! `asop.json`; writing them into the language file of the game is for the
//! host to do ([`ShipArea::on_names`]).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::asop;
use crate::basetool::area_support::{
    Posted, pull_generic, put_object, send_unless_held, store_busy, store_ok, take_object, texts,
};
use crate::basetool::error::ApiError;
use crate::basetool::feed;
use crate::basetool::orgunits::OrgUnit;
use crate::basetool::scopes::{ROUTE_SHIPS, SETTING_NEW_SHIP_UNIT, SETTING_SHIPS};
use crate::basetool::ship_sync::{self, LocalShip, Plan, ShipState};
use crate::basetool::sync::{Area, Pass, Shared};
use crate::hangar;
use crate::jsonio::{Map, Value, map_set};
use crate::paths::Paths;

/// Refusals where the Basetool wins over the unit: Verse-Kit takes its unit
/// instead of sending again at every sync.
pub const UNIT_REJECTIONS: [&str; 2] = ["ORG_UNIT_NOT_ALLOWED", "ORG_UNIT_VERSION_CONFLICT"];

/// The mark of an entry whose insurance comes from the Basetool (later imports
/// do not overwrite it).
const BASETOOL_COVER: &str = "versicherung_basetool";

/// `{hangar name: (key, own name)}` for all ships that match a game name;
/// empty without a language file (`_own_ship_names`).
fn own_ship_names(paths: &Paths, data: &Map) -> BTreeMap<String, (String, String)> {
    let ships = hangar::ships(data);
    if ships.is_empty() {
        return BTreeMap::new();
    }
    let table = asop::factory_table(paths);
    if table.is_empty() {
        return BTreeMap::new();
    }
    let stand = asop::load(&paths.app_file(asop::FILE));
    asop::match_ships(&ships, &table)
        .into_iter()
        .filter(|m| !m.key.is_empty())
        .map(|m| {
            let own = asop::entry(&stand, &m.key).0;
            (m.name, (m.key, own))
        })
        .collect()
}

fn insurance_of(ship: &Value) -> Value {
    match ship.get("insurance") {
        Some(v @ Value::Object(_)) => v.clone(),
        _ => Value::Object(Map::new()),
    }
}

/// The area.
pub struct ShipArea {
    /// The units of the member, for the choice of the unit.
    pub units: Shared<Vec<OrgUnit>>,
    /// Called when own names were written into `asop.json`: the host writes
    /// them into the language file of the game.
    pub on_names: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ShipArea {
    pub fn new(units: Shared<Vec<OrgUnit>>) -> ShipArea {
        ShipArea { units, on_names: None }
    }

    /// The unit for new ships from the setting; only with a fetched list, and only a
    /// unit the member is still in (a left one would give `ORG_UNIT_NOT_ALLOWED` for each new ship).
    fn new_unit(paths: &Paths, known: Option<&BTreeSet<String>>) -> Option<String> {
        let known = known?;
        let wanted = paths.setting(SETTING_NEW_SHIP_UNIT).unwrap_or_default();
        let wanted = crate::text::py_strip(&wanted);
        (!wanted.is_empty() && known.contains(wanted)).then(|| wanted.to_string())
    }

    /// Takes over in the hangar (`_apply_ships`): ships from over there, insurance, ready mark and
    /// unit of linked ships, ships gone over there that came from there. Gives the own
    /// names that came in.
    fn apply(
        pass: &Pass<'_>,
        plan: &Plan,
        server: &BTreeMap<String, Value>,
        local: &BTreeMap<String, LocalShip>,
        unit_clear: &[String],
    ) -> Result<Vec<(String, String)>, ApiError> {
        let path = pass.paths.app_file(hangar::FILE);
        let mut names: Vec<(String, String)> = Vec::new();
        pass.apply_local(|| {
            let _lock = hangar::lock();
            // One read; a file that cannot be read now is no empty hangar (see `store_ok`).
            let Some(mut data) = hangar::load_checked(&path) else {
                return Err(store_busy("hangar unreadable"));
            };
            let by_ext: BTreeMap<String, usize> = hangar::ships(&data)
                .iter()
                .enumerate()
                .map(|(index, entry)| (ship_sync::external_id(entry), index))
                .collect();
            // Ships from over there come in first; they are put at the end, so the places in `by_ext` hold.
            for id in &plan.take {
                let ship = server.get(id);
                let name = ship
                    .and_then(|s| s.get("shipType"))
                    .and_then(|t| t.get("name"))
                    .and_then(Value::as_str)
                    .map(|n| crate::text::py_strip(n).to_string())
                    .unwrap_or_default();
                if name.is_empty() {
                    continue;
                }
                let cover = ship.map(insurance_of).unwrap_or(Value::Object(Map::new()));
                let lti = cover.get("kind").and_then(Value::as_str) == Some(ship_sync::LTI);
                let months = if cover.get("kind").and_then(Value::as_str) == Some(ship_sync::MONTHS) {
                    cover.get("months").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                };
                let mut rest = Map::new();
                map_set(&mut rest, "lti", Value::Bool(lti));
                map_set(&mut rest, "versicherung", months);
                hangar::add_ship(&mut data, &name, "", hangar::BASETOOL, &rest);
            }
            hangar::edit_ships(&mut data, |ships| {
                for (ext, id) in &plan.pull {
                    let ship = server.get(id);
                    let fields: Vec<String> =
                        plan.pull_fields.get(ext).cloned().unwrap_or_else(|| vec!["insurance".into()]);
                    let Some(index) = by_ext.get(ext).copied() else { continue };
                    let Some(entry) = ships.get_mut(index) else { continue };
                    let cover = ship.map(insurance_of).unwrap_or(Value::Object(Map::new()));
                    if fields.iter().any(|f| f == "insurance") {
                        let lti = cover.get("kind").and_then(Value::as_str) == Some(ship_sync::LTI);
                        let months = if cover.get("kind").and_then(Value::as_str) == Some(ship_sync::MONTHS) {
                            cover.get("months").cloned().unwrap_or(Value::Null)
                        } else {
                            Value::Null
                        };
                        map_set(entry, "lti", Value::Bool(lti));
                        map_set(entry, "versicherung", months);
                        // The insurance of the Basetool is changed by no import any more.
                        if ship_sync::covered(Some(&cover)) {
                            map_set(entry, BASETOOL_COVER, Value::Bool(true));
                        } else {
                            entry.retain(|(k, _)| k != BASETOOL_COVER);
                        }
                    }
                    if fields.iter().any(|f| f == "fitted") {
                        let fitted = ship.and_then(|s| s.get("fitted")).is_some_and(crate::basetool::error::truthy);
                        map_set(entry, "einsatzbereit", Value::Bool(fitted));
                    }
                    if fields.iter().any(|f| f == "orgUnit") {
                        let unit = ship.map(ship_sync::server_unit).unwrap_or_default();
                        map_set(entry, "einheit", Value::str(unit));
                    }
                    let key = local.get(ext).and_then(|l| l.asop_key.clone());
                    if fields.iter().any(|f| f == "name")
                        && let Some(key) = key
                    {
                        let name =
                            ship.and_then(|s| s.get("name")).and_then(Value::as_str).unwrap_or_default().to_string();
                        names.push((key, name));
                    }
                }
                for ext in unit_clear {
                    if let Some(entry) = by_ext.get(ext).and_then(|i| ships.get_mut(*i)) {
                        entry.retain(|(k, _)| k != "einheit");
                    }
                }
                let doomed: BTreeSet<usize> = plan
                    .drop
                    .iter()
                    .filter_map(|ext| by_ext.get(ext).copied())
                    .filter(|i| ships.get(*i).is_some_and(|e| hangar::field_text(e, "herkunft") == hangar::BASETOOL))
                    .collect();
                if !doomed.is_empty() {
                    let kept: Vec<Map> =
                        ships.drain(..).enumerate().filter(|(i, _)| !doomed.contains(i)).map(|(_, e)| e).collect();
                    *ships = kept;
                }
            });
            if hangar::save(&path, &mut data) { Ok(()) } else { Err(store_busy("hangar not written")) }
        })?;
        Ok(names)
    }

    /// Own names changed over there go into `asop.json` (the star stays as it was).
    fn take_names(&self, pass: &Pass<'_>, names: &[(String, String)]) {
        if names.is_empty() {
            return;
        }
        let path = pass.paths.app_file(asop::FILE);
        let mut written = false;
        pass.apply_local(|| {
            for (key, name) in names {
                let (_, star) = asop::entry(&asop::load(&path), key);
                let (stored, _) = asop::store(&path, key, name, star);
                written |= matches!(stored, asop::Stored::Written | asop::Stored::SetAside);
            }
        });
        if written && let Some(hook) = &self.on_names {
            hook();
        }
    }

    fn sync(&self, pass: &mut Pass<'_>, sub: &mut Map) -> Result<Map, ApiError> {
        let path = pass.paths.app_file(hangar::FILE);
        let (mut server, _resync) = pull_generic(pass.conn, sub, "/me/ships", "shipId")?;
        // One read, no check before it: a file that cannot be read now must not count as an empty hangar.
        let Some(data) = hangar::load_checked(&path) else { return Err(store_busy("hangar unreadable")) };
        let names = own_ship_names(pass.paths, &data);
        let local = ship_sync::local_ships(&hangar::ships(&data), &names);
        let refs: Vec<(String, Value)> =
            local.iter().map(|(ext, ship)| (ext.clone(), Value::Object(ship.reference.clone()))).collect();
        let mut cache = take_object(sub, "resolved");
        let found = feed::resolve_refs(pass.conn, "SHIP_TYPE", &refs, &mut cache, pass.now);
        put_object(sub, "resolved", cache);
        let resolved: BTreeMap<String, String> = found?.into_iter().collect();
        let state = ShipState::from_value(&Value::Object(sub.clone()));
        let mut result = ship_sync::plan(&local, &resolved, &server, &state);

        // The units of the member: only a fetched list counts.
        let units = self.units.get().1;
        let known: Option<BTreeSet<String>> = units.as_ref().map(|list| list.iter().map(|u| u.id.clone()).collect());
        let new_unit = Self::new_unit(pass.paths, known.as_ref());
        let sets =
            ship_sync::change_sets(&result, &local, &resolved, &server, 500, new_unit.as_deref(), known.as_ref());
        let Posted { failed, applied, posted, paused, detached, waiting, .. } =
            send_unless_held(pass, ROUTE_SHIPS, &sets)?;
        let rejected: BTreeMap<String, String> = failed
            .iter()
            .map(|(ext, kind, reason)| (ext.clone(), if reason.is_empty() { kind.clone() } else { reason.clone() }))
            .collect();
        // With a lock only what stood in an accepted send counts.
        if paused {
            let posted: BTreeSet<&String> = posted.iter().collect();
            result.link.retain(|(ext, _)| posted.contains(ext));
            result.create.retain(|ext| posted.contains(ext));
            result.update.retain(|(ext, _)| posted.contains(ext));
            result.remove.retain(|(ext, _)| posted.contains(ext));
            result.retain_overrides(&posted);
        }
        // The insurance fetched with linking counts only when the `link` was accepted.
        let linked: BTreeSet<&String> =
            result.link.iter().map(|(ext, _)| ext).filter(|e| !rejected.contains_key(*e)).collect();
        let link_pull = result.link_pull.clone();
        result.pull.retain(|(ext, _)| !link_pull.contains(ext) || linked.contains(ext));
        // A refused unit: the unit of the Basetool is taken (see `UNIT_REJECTIONS`).
        let mut unit_reset: Vec<String> = rejected
            .iter()
            .filter(|(ext, reason)| UNIT_REJECTIONS.contains(&reason.as_str()) && result.links.contains_key(*ext))
            .map(|(ext, _)| ext.clone())
            .collect();
        unit_reset.sort();
        for ext in &unit_reset {
            if !result.pull.iter().any(|(e, _)| e == ext) {
                result.pull.push((ext.clone(), result.links[ext].clone()));
            }
            let mut fields: BTreeSet<String> =
                result.pull_fields.get(ext).cloned().unwrap_or_default().into_iter().collect();
            fields.insert("orgUnit".into());
            result.pull_fields.insert(ext.clone(), fields.into_iter().collect());
        }
        let mut elsewhere: BTreeSet<String> = state.removed_elsewhere.clone();
        elsewhere.extend(result.removed_elsewhere.iter().cloned());
        elsewhere.extend(rejected.iter().filter(|(_, r)| r.as_str() == "REMOVED_ELSEWHERE").map(|(e, _)| e.clone()));

        if applied > 0 {
            map_set(sub, "server", Value::Object(server.iter().map(|(k, v)| (k.clone(), v.clone())).collect()));
            server = pull_generic(pass.conn, sub, "/me/ships", "shipId")?.0;
        }

        // A unit left out when making (unknown or no list): back to "as in the Basetool" in the hangar, or it
        // would go out as a change one sync later after all.
        let mut unit_clear: Vec<String> = result
            .create
            .iter()
            .filter(|ext| {
                local
                    .get(*ext)
                    .and_then(|l| l.unit.as_ref())
                    .is_some_and(|unit| !(known.as_ref().is_some_and(|k| unit.is_empty() || k.contains(unit))))
            })
            .cloned()
            .collect();
        unit_clear.sort();
        if !result.take.is_empty() || !result.drop.is_empty() || !result.pull.is_empty() || !unit_clear.is_empty() {
            let names = Self::apply(pass, &result, &server, &local, &unit_clear)?;
            self.take_names(pass, &names);
        }

        let mut links: BTreeMap<String, String> = BTreeMap::new();
        for (ship_id, ship) in &server {
            if let Some(ext) = ship.get("externalId").and_then(Value::as_str).filter(|e| !e.is_empty()) {
                links.insert(ext.to_string(), ship_id.clone());
            }
        }
        for (ext, ship_id) in &result.local_removed {
            links.entry(ext.clone()).or_insert_with(|| ship_id.clone());
        }
        let (mut baseline, mut baseline_name, mut baseline_fitted, mut baseline_unit) =
            (Map::new(), Map::new(), Map::new(), Map::new());
        for (ext, ship_id) in &links {
            if let Some(ship) = server.get(ship_id) {
                map_set(&mut baseline, ext, ship.get("insurance").cloned().unwrap_or(Value::Null));
                let name =
                    ship.get("name").filter(|n| crate::basetool::error::truthy(n)).map(crate::basetool::error::py_str);
                map_set(&mut baseline_name, ext, Value::str(name.unwrap_or_default()));
                map_set(
                    &mut baseline_fitted,
                    ext,
                    Value::Bool(ship.get("fitted").is_some_and(crate::basetool::error::truthy)),
                );
                map_set(&mut baseline_unit, ext, Value::str(ship_sync::server_unit(ship)));
            }
        }
        let done: BTreeSet<String> = result.remove.iter().map(|(ext, _)| ext.clone()).collect();
        let created: BTreeSet<String> = result.create.iter().filter(|e| !rejected.contains_key(*e)).cloned().collect();
        // Releases that are done over there already (a send confirmed in the browser): removed is what is no
        // longer in the stand; taken back is what is linked again.
        let old_pending: BTreeSet<String> = texts(sub, "pending_removals").into_iter().collect();
        let pending: BTreeSet<String> = old_pending
            .difference(&done)
            .filter(|ext| links.get(*ext).is_some_and(|id| server.contains_key(id)))
            .cloned()
            .collect();
        let old_overrides: BTreeSet<String> = texts(sub, "overrides").into_iter().collect();
        let sent_override: BTreeSet<&String> = result.r#override_.iter().collect();
        let overrides: BTreeSet<String> = old_overrides
            .iter()
            .filter(|e| !sent_override.contains(*e))
            .filter(|e| !links.get(*e).is_some_and(|id| server.contains_key(id)))
            .cloned()
            .collect();
        let removed_elsewhere: Vec<String> = elsewhere.difference(&created).cloned().collect();
        let local_removed: Map = result
            .local_removed
            .iter()
            .filter(|(ext, id)| server.contains_key(*id) && !done.contains(*ext))
            .map(|(k, v)| (k.clone(), Value::str(v)))
            .collect();
        let rejected_state: Map = rejected
            .iter()
            .filter(|(ext, _)| !unit_reset.contains(ext))
            .map(|(k, v)| (k.clone(), Value::str(v)))
            .collect();
        let list = |items: &mut dyn Iterator<Item = &String>| Value::Array(items.map(Value::str).collect());
        map_set(sub, "server", Value::Object(server.iter().map(|(k, v)| (k.clone(), v.clone())).collect()));
        map_set(sub, "links", Value::Object(links.iter().map(|(k, v)| (k.clone(), Value::str(v))).collect()));
        map_set(sub, "baseline", Value::Object(baseline));
        map_set(sub, "baseline_name", Value::Object(baseline_name));
        map_set(sub, "baseline_fitted", Value::Object(baseline_fitted));
        map_set(sub, "baseline_unit", Value::Object(baseline_unit));
        map_set(sub, "removed_elsewhere", list(&mut removed_elsewhere.iter()));
        map_set(sub, "local_removed", Value::Object(local_removed));
        map_set(sub, "pending_removals", list(&mut pending.iter()));
        map_set(sub, "overrides", list(&mut overrides.iter()));
        map_set(sub, "rejected", Value::Object(rejected_state));
        map_set(sub, "names", Value::Object(local.iter().map(|(ext, l)| (ext.clone(), Value::str(&l.name))).collect()));
        // `kept` stays as it is (the player's "stay in the Basetool").
        let rejected_count = rejected.keys().filter(|e| !unit_reset.contains(e)).count();
        let unit_taken =
            result.unit_conflicts.iter().collect::<BTreeSet<_>>().iter().filter(|e| !unit_reset.contains(**e)).count();
        let mut counts = Map::new();
        for (name, value) in [
            ("linked", result.link.len() as i64),
            ("created", created.len() as i64),
            ("updated", result.update.len() as i64),
            ("taken", result.take.len() as i64),
            ("removed", done.len() as i64),
            ("rejected", rejected_count as i64),
            ("unit_taken", unit_taken as i64),
            ("unit_refused", unit_reset.len() as i64),
            ("unresolved", local.keys().filter(|e| !resolved.contains_key(*e)).count() as i64),
            ("detached", detached),
            ("paused", i64::from(paused)),
            ("waiting", i64::from(waiting.is_some())),
        ] {
            map_set(&mut counts, name, Value::int(value));
        }
        Ok(counts)
    }
}

impl Plan {
    /// Keeps only the agreed creations whose identifier stood in an accepted send.
    fn retain_overrides(&mut self, posted: &BTreeSet<&String>) {
        self.override_.retain(|ext| posted.contains(ext));
    }
}

impl Area for ShipArea {
    fn name(&self) -> &'static str {
        "ships"
    }

    fn runs(&self, ready: &[&'static str], _capabilities: &[String]) -> bool {
        ready.contains(&SETTING_SHIPS)
    }

    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError> {
        if !store_ok(&pass.paths.app_file(hangar::FILE), "schiffe") {
            crate::errors::record("basetool_sync.hangar", &"Hangardatei unlesbar, Hangar wird nicht abgeglichen");
            return Ok(vec![("unreadable".into(), Value::int(1))]);
        }
        let mut sub = take_object(pass.state, "ships");
        let result = self.sync(pass, &mut sub);
        put_object(pass.state, "ships", sub);
        result
    }

    fn forget(&mut self) {}
}
