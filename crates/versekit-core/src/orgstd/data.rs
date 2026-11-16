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

//! What an org server answers to the three lists of the standard (`/stock`,
//! `/blueprints`, `/orders`), checked row by row: a row that breaks the
//! standard falls away and is counted, never the whole page.
//!
//! ⚠⚠ The rows are the data of other members: they only ever live in memory
//! (see [`crate::orgstd::hub`]), never in a file, never in the backup.

use serde_json::Value;

/// The longest text of a name, a place and a title taken over; the standard allows 200.
const TEXT_MAX: usize = 200;
/// The longest member name taken over.
const MEMBER_MAX: usize = 80;
/// The longest note of an order taken over; the standard allows 1000.
const NOTE_MAX: usize = 1000;
/// The largest quantity taken over; the same bound as the template server.
pub const QUANTITY_MAX: f64 = 1e9;
/// The highest quality of the standard.
pub const QUALITY_MAX: i64 = 1000;

/// The unit of a quantity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Standard cargo units.
    Scu,
    /// Hundredths of a cargo unit.
    Cscu,
    /// Whole pieces; also what a missing or unknown unit means.
    Piece,
}

impl Unit {
    /// The word of the standard.
    pub fn code(self) -> &'static str {
        match self {
            Unit::Scu => "scu",
            Unit::Cscu => "cscu",
            Unit::Piece => "piece",
        }
    }

    fn of(text: Option<&str>) -> Unit {
        match text.map(str::to_ascii_lowercase).as_deref() {
            Some("scu") => Unit::Scu,
            Some("cscu") => Unit::Cscu,
            _ => Unit::Piece,
        }
    }
}

/// One row of the stock the org shows.
#[derive(Clone, Debug, PartialEq)]
pub struct OrgLot {
    pub item: String,
    pub quantity: f64,
    pub unit: Unit,
    pub quality: Option<u16>,
    pub location: String,
    /// The member who shares it; empty when the server does not say.
    pub member: String,
}

/// One blueprint of the org with who holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrgBlueprint {
    pub name: String,
    pub holders: Vec<String>,
}

/// The state of an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderStatus {
    Open,
    Done,
    Cancelled,
}

/// One good of an order.
#[derive(Clone, Debug, PartialEq)]
pub struct OrderLine {
    pub item: String,
    pub quantity: f64,
    pub unit: Unit,
    pub delivered: f64,
}

impl OrderLine {
    /// What is still missing; never below zero.
    pub fn open(&self) -> f64 {
        (self.quantity - self.delivered).max(0.0)
    }
}

/// One order of the org.
#[derive(Clone, Debug, PartialEq)]
pub struct OrgOrder {
    pub id: String,
    pub title: String,
    pub status: OrderStatus,
    pub location: String,
    /// `YYYY-MM-DD`, or empty.
    pub due: String,
    pub note: String,
    pub lines: Vec<OrderLine>,
}

/// The rows that passed and how many did not.
#[derive(Clone, Debug, PartialEq)]
pub struct Parsed<T> {
    pub rows: Vec<T>,
    pub dropped: usize,
}

impl<T> Default for Parsed<T> {
    fn default() -> Self {
        Parsed { rows: Vec::new(), dropped: 0 }
    }
}

/// `text` without control characters, at most `most` characters; `None` when nothing is left.
fn clean(text: &str, most: usize) -> Option<String> {
    let cleaned: String = text.chars().filter(|c| !c.is_control()).take(most).collect();
    let cleaned = cleaned.trim().to_string();
    if cleaned.is_empty() { None } else { Some(cleaned) }
}

fn text_of(row: &Value, key: &str, most: usize) -> Option<String> {
    row.get(key).and_then(Value::as_str).and_then(|t| clean(t, most))
}

/// A finite number from `0` to [`QUANTITY_MAX`].
fn quantity_of(row: &Value, key: &str) -> Option<f64> {
    row.get(key).and_then(Value::as_f64).filter(|q| q.is_finite() && (0.0..=QUANTITY_MAX).contains(q))
}

/// The rows of `/stock`.
pub fn parse_lots(values: &[Value]) -> Parsed<OrgLot> {
    let mut parsed = Parsed::default();
    for row in values {
        let quality = match row.get("quality") {
            None | Some(Value::Null) => Some(None),
            Some(q) => q.as_i64().filter(|q| (0..=QUALITY_MAX).contains(q)).map(|q| Some(q as u16)),
        };
        let (Some(item), Some(quantity), Some(quality)) =
            (text_of(row, "item", TEXT_MAX), quantity_of(row, "quantity"), quality)
        else {
            parsed.dropped += 1;
            continue;
        };
        parsed.rows.push(OrgLot {
            item,
            quantity,
            unit: Unit::of(row.get("unit").and_then(Value::as_str)),
            quality,
            location: text_of(row, "location", TEXT_MAX).unwrap_or_default(),
            member: crate::streamer::private(text_of(row, "member", MEMBER_MAX).unwrap_or_default()),
        });
    }
    parsed
}

/// The rows of `/blueprints`.
pub fn parse_blueprints(values: &[Value]) -> Parsed<OrgBlueprint> {
    let mut parsed = Parsed::default();
    for row in values {
        let Some(name) = text_of(row, "name", TEXT_MAX) else {
            parsed.dropped += 1;
            continue;
        };
        let holders = row
            .get("holders")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|h| clean(h, MEMBER_MAX))
            .map(crate::streamer::private)
            .collect();
        parsed.rows.push(OrgBlueprint { name, holders });
    }
    parsed
}

fn date_ok(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10 && b.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
}

/// The rows of `/orders`.
pub fn parse_orders(values: &[Value]) -> Parsed<OrgOrder> {
    let mut parsed = Parsed::default();
    for row in values {
        let status = match row.get("status").and_then(Value::as_str) {
            Some("open") => Some(OrderStatus::Open),
            Some("done") => Some(OrderStatus::Done),
            Some("cancelled") => Some(OrderStatus::Cancelled),
            _ => None,
        };
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 200 && !id.contains(char::is_control));
        let lines: Option<Vec<OrderLine>> = row.get("items").and_then(Value::as_array).map(|items| {
            items
                .iter()
                .filter_map(|line| {
                    Some(OrderLine {
                        item: text_of(line, "item", TEXT_MAX)?,
                        quantity: quantity_of(line, "quantity")?,
                        unit: Unit::of(line.get("unit").and_then(Value::as_str)),
                        delivered: quantity_of(line, "delivered")?,
                    })
                })
                .collect()
        });
        let all_lines = row.get("items").and_then(Value::as_array).map_or(0, Vec::len);
        let (Some(id), Some(title), Some(status), Some(lines)) = (id, text_of(row, "title", TEXT_MAX), status, lines)
        else {
            parsed.dropped += 1;
            continue;
        };
        // An order with a line that breaks the standard is not shown: its progress would be wrong.
        if lines.len() != all_lines {
            parsed.dropped += 1;
            continue;
        }
        parsed.rows.push(OrgOrder {
            id: id.to_string(),
            title,
            status,
            location: text_of(row, "location", TEXT_MAX).unwrap_or_default(),
            due: row.get("due").and_then(Value::as_str).filter(|d| date_ok(d)).unwrap_or_default().to_string(),
            note: text_of(row, "note", NOTE_MAX).unwrap_or_default(),
            lines,
        });
    }
    parsed
}

#[cfg(test)]
mod tests;
