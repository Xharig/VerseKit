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

//! What of the own stock and the own blueprints goes to an org: pure
//! functions without a network, so the page can show the member exactly what
//! would go out before anything does.
//!
//! The standard only knows "replace the whole list". So nothing here sends:
//! it builds the list, counts what is left out and why, and refuses a list
//! that is too long instead of cutting it. The purchase price, the "stolen"
//! mark and every other field of the stock never enter a [`StockItem`].

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::orgstd::data::Unit;
use crate::orgstd::{E_EMPTY, E_LIMIT, OrgError};
use crate::stock::Row;
use crate::stock_data::StockData;

/// The most lines of the stock the standard takes.
pub const STOCK_MAX: usize = 5000;
/// The most blueprints the standard takes.
pub const BLUEPRINTS_MAX: usize = 10_000;
/// The longest name and place the standard takes.
pub const TEXT_MAX: usize = 200;
/// The highest quality of the standard.
const QUALITY_MAX: i64 = 1000;

/// One line that goes out (`StockItem` of the standard).
#[derive(Clone, Debug, PartialEq)]
pub struct StockItem {
    pub item: String,
    pub quantity: f64,
    pub unit: Unit,
    pub quality: Option<u16>,
    pub location: String,
}

impl StockItem {
    /// The line as the request carries it.
    pub fn to_json(&self) -> Value {
        let mut line = json!({ "item": self.item, "quantity": self.quantity, "unit": self.unit.code() });
        if let Some(quality) = self.quality {
            line["quality"] = json!(quality);
        }
        if !self.location.is_empty() {
            line["location"] = json!(self.location);
        }
        line
    }
}

/// Why lines of the stock stay home, counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Left {
    /// The name is not one VerseKit knows for sure as the English name of the game.
    pub unknown: usize,
    /// Marked as stolen: the standard has no such field, the org would read it as normal goods.
    pub stolen: usize,
    /// No quantity left.
    pub empty: usize,
    /// Lines whose place was cut to the length of the standard (they do go out).
    pub shortened: usize,
}

/// The list for `PUT /stock/mine` and what was left out of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StockShare {
    pub items: Vec<StockItem>,
    pub left: Left,
    /// Lines of the own stock that were summed into another.
    pub merged: usize,
    /// More lines than the standard takes: the number; the list is then not sent.
    pub too_many: Option<usize>,
}

impl StockShare {
    /// Why this list may not be sent, or `None` when it may: a list that is too long is never cut, an empty one
    /// is never sent from here (ending the sharing is a way of its own).
    pub fn refusal(&self) -> Option<OrgError> {
        if self.too_many.is_some() {
            Some(OrgError::new(E_LIMIT))
        } else if self.items.is_empty() {
            Some(OrgError::new(E_EMPTY))
        } else {
            None
        }
    }

    /// The lines as the request carries them.
    pub fn to_json(&self) -> Vec<Value> {
        self.items.iter().map(StockItem::to_json).collect()
    }
}

/// What makes two lines of the stock one: the name, the quality, the place and the unit.
type LineKey = (String, Option<u16>, String, &'static str);

fn clipped(text: &str) -> (String, bool) {
    let cleaned: String = text.chars().filter(|c| !c.is_control()).collect();
    let cleaned = cleaned.trim();
    let cut = cleaned.chars().count() > TEXT_MAX;
    (cleaned.chars().take(TEXT_MAX).collect(), cut)
}

/// Quantity as the standard wants it: SCU with three decimals, pieces whole.
fn rounded(quantity: f64, unit: Unit) -> f64 {
    match unit {
        Unit::Piece => quantity.round(),
        _ => (quantity * 1000.0).round() / 1000.0,
    }
}

/// The English name of `row` as the lists of the program know it, or `None` when it is not sure.
fn known_name(row: &Row, data: &StockData) -> Option<String> {
    if row.store == crate::stock::Store::Raw {
        return data.storage_name(&row.name);
    }
    if row.item {
        return data.item_choice(&row.name).map(|choice| choice.name.clone());
    }
    data.knows_goods(&row.name).then(|| row.name.clone())
}

/// Builds the list of the own stock for an org. Only the fields of a [`StockItem`] are read: never the purchase
/// price, the UEX number or anything else of a row.
pub fn stock_share(rows: &[Row], data: &StockData) -> StockShare {
    let mut share = StockShare::default();
    // Equal lines are summed so the limit of the standard reaches; the key is what makes two lines equal.
    let mut sums: BTreeMap<LineKey, (f64, bool)> = BTreeMap::new();
    for row in rows {
        if row.stolen {
            share.left.stolen += 1;
            continue;
        }
        let Some(name) = known_name(row, data).map(|n| clipped(&n).0).filter(|n| !n.is_empty()) else {
            share.left.unknown += 1;
            continue;
        };
        let unit = if row.piece { Unit::Piece } else { Unit::Scu };
        let quantity = rounded(row.amount, unit);
        if !quantity.is_finite() || quantity <= 0.0 {
            share.left.empty += 1;
            continue;
        }
        let quality = row.quality.filter(|q| (0..=QUALITY_MAX).contains(q)).map(|q| q as u16);
        let (place, cut) = clipped(&row.place);
        let entry = sums.entry((name, quality, place, unit.code())).or_insert((0.0, false));
        if entry.0 > 0.0 {
            share.merged += 1;
        }
        entry.0 += quantity;
        entry.1 = entry.1 || cut;
    }
    for ((item, quality, location, unit), (quantity, cut)) in sums {
        let unit = if unit == Unit::Piece.code() { Unit::Piece } else { Unit::Scu };
        if cut {
            share.left.shortened += 1;
        }
        share.items.push(StockItem { item, quantity: rounded(quantity, unit), unit, quality, location });
    }
    if share.items.len() > STOCK_MAX {
        share.too_many = Some(share.items.len());
    }
    share
}

/// The list for `PUT /blueprints/mine`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlueprintShare {
    pub names: Vec<String>,
    /// Names that appeared twice (also in another spelling of the case).
    pub twice: usize,
    /// Names without any text.
    pub empty: usize,
    /// Names the catalog of the game data does not know: not sure to be the English name, so not shared.
    pub unknown: usize,
    /// More names than the standard takes: the number; the list is then not sent.
    pub too_many: Option<usize>,
}

/// The list of the own blueprints for an org out of the entries of the collection (name, source): the starting
/// blueprints stay home, a name goes out in the spelling of the catalog of the game data (English) and only when the
/// catalog knows it; the others are counted in `unknown`.
pub fn own_blueprint_share(
    entries: impl IntoIterator<Item = (String, String)>,
    catalog: &crate::catalog_cache::CatalogCache,
) -> BlueprintShare {
    let known: std::collections::HashMap<&str, &str> =
        catalog.blueprints.iter().map(|b| (b.key.as_str(), b.name.as_str())).collect();
    let mut names = Vec::new();
    let mut unknown = 0;
    for (name, source) in entries {
        if crate::basetool::blueprint_sync::NEVER_SENT.contains(&source.as_str()) {
            continue;
        }
        match known.get(crate::paths::name_key(&name).as_str()) {
            Some(catalog_name) => names.push((*catalog_name).to_string()),
            None => unknown += 1,
        }
    }
    BlueprintShare { unknown, ..blueprint_share(names) }
}

impl BlueprintShare {
    /// As [`StockShare::refusal`].
    pub fn refusal(&self) -> Option<OrgError> {
        if self.too_many.is_some() {
            Some(OrgError::new(E_LIMIT))
        } else if self.names.is_empty() {
            Some(OrgError::new(E_EMPTY))
        } else {
            None
        }
    }
}

/// Builds the list of the own blueprints, in the spelling of the list, each once; the starting blueprints are
/// not in `names`.
pub fn blueprint_share(names: impl IntoIterator<Item = String>) -> BlueprintShare {
    let mut share = BlueprintShare::default();
    let mut seen = std::collections::HashSet::new();
    for name in names {
        let (name, _) = clipped(&name);
        if name.is_empty() {
            share.empty += 1;
        } else if !seen.insert(name.to_lowercase()) {
            share.twice += 1;
        } else {
            share.names.push(name);
        }
    }
    if share.names.len() > BLUEPRINTS_MAX {
        share.too_many = Some(share.names.len());
    }
    share
}

/// A key for one booking: 128 random bits as 32 hexadecimal digits. A booking sent again after a network error
/// carries the same key.
pub fn idempotency_key() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        // Without a random source the time and a counter still give a key no earlier booking of this run has.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        bytes = (nanos ^ (u128::from(count) << 64)).to_le_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests;
