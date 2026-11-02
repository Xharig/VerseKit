// Verse-Kit — companion for Star Citizen.
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

//! The units of the member (`GET /me/org-units`, right
//! `exchange.orgunits.read`; `exchange_orgunits.py`). Needed for the unit of
//! a ship and for the pool a share from "My stock" goes into.
//!
//! ⚠ In memory only, fetched anew with every sync: a membership can begin
//! or end at any time. The id is opaque.

use crate::basetool::connection::Connection;
use crate::basetool::error::{ApiError, py_str, truthy};
use crate::basetool::feed::list;
use crate::basetool::scopes::{SCOPES_ORGUNITS, SETTING_SHIPS, SETTING_STOCK, all_in};
use crate::basetool::sync::{Area, Pass, Shared};
use crate::jsonio::{Map, Value};

pub const PATH: &str = "/me/org-units";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrgUnit {
    pub id: String,
    pub name: String,
    pub shorthand: String,
    /// Unknown kinds stay selectable.
    pub kind: String,
}

/// `str(item.get(key) or '').strip()`.
fn text(item: &Value, key: &str) -> String {
    match item.get(key) {
        Some(value) if truthy(value) => crate::text::py_strip(&py_str(value)).to_string(),
        _ => String::new(),
    }
}

/// `{items: [{id, name, shorthand, kind}]}` to units; one without `id`
/// is worthless and dropped.
pub fn parse(answer: &Map) -> Vec<OrgUnit> {
    list(answer, "items")
        .iter()
        .filter(|item| matches!(item, Value::Object(_)))
        .filter_map(|item| {
            let id = text(item, "id");
            (!id.is_empty()).then(|| OrgUnit {
                id,
                name: crate::streamer::private(text(item, "name")),
                shorthand: crate::streamer::private(text(item, "shorthand")),
                kind: text(item, "kind"),
            })
        })
        .collect()
}

/// How a unit is named in a choice: `Staffel Alpha (ALP)`.
pub fn label(unit: &OrgUnit) -> String {
    let name = [&unit.name, &unit.shorthand, &unit.id].into_iter().find(|s| !s.is_empty()).cloned().unwrap_or_default();
    if !unit.shorthand.is_empty() && unit.shorthand != name { format!("{name} ({})", unit.shorthand) } else { name }
}

/// Fetches the units of the member.
pub fn fetch(conn: &Connection) -> Result<Vec<OrgUnit>, ApiError> {
    Ok(parse(&conn.request("GET", PATH, None, &[], None)?))
}

/// The area: runs with hangar or stock, if the right is granted. Without the
/// list only the choice is missing, so its errors stop nothing else.
#[derive(Clone, Default)]
pub struct OrgUnitsArea {
    pub units: Shared<Vec<OrgUnit>>,
}

impl Area for OrgUnitsArea {
    fn name(&self) -> &'static str {
        "orgunits"
    }

    fn runs(&self, ready: &[&'static str], capabilities: &[String]) -> bool {
        (ready.contains(&SETTING_SHIPS) || ready.contains(&SETTING_STOCK)) && all_in(SCOPES_ORGUNITS, capabilities)
    }

    fn isolated(&self) -> bool {
        true
    }

    /// An old list would offer a unit the member may have left: gone after
    /// every error.
    fn forget_after(&self, _action: crate::basetool::error::Action) -> bool {
        true
    }

    fn run(&mut self, pass: &mut Pass<'_>) -> Result<Map, ApiError> {
        let units = fetch(pass.conn)?;
        let count = units.len();
        let unchanged = self.units.get().1.is_some_and(|known| *known == units);
        if !unchanged {
            self.units.publish(units);
        }
        Ok(vec![("units".into(), Value::int(count as i64))])
    }

    fn forget(&mut self) {
        if self.units.get().1.is_some() {
            self.units.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_and_labels() {
        let answer = crate::basetool::connection::parse_answer(
            br#"{"items": [{"id": " u1 ", "name": "Staffel Alpha", "shorthand": "ALP", "kind": "SQUADRON"},
                           {"name": "ohne Kennung"}, "kaputt",
                           {"id": "u2", "name": "Mamba", "shorthand": "Mamba", "kind": "NEU"}]}"#,
        );
        let units = parse(&answer);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].id, "u1");
        assert_eq!(label(&units[0]), "Staffel Alpha (ALP)");
        assert_eq!(label(&units[1]), "Mamba");
        let bare = OrgUnit { id: "x".into(), name: String::new(), shorthand: String::new(), kind: String::new() };
        assert_eq!(label(&bare), "x");
    }
}
