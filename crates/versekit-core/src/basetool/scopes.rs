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

//! The areas, their switches and the rights they need
//! (`basetool.SCOPES_*`, `basetool_sync.AREAS`, `wanted_scopes`).
//!
//! Every area is off by default, and only rights of switched-on areas are
//! asked for. ⚠⚠ Only ask for what the Basetool has released for Verse-Kit:
//! a right it does not know can make the whole login fail.

/// Always: the connection itself, and staying connected after the player
/// logs out in the browser.
pub const SCOPE_CONNECT: &str = "exchange.connect";
pub const SCOPE_OFFLINE: &str = "offline_access";

pub const SCOPES_BLUEPRINTS: &[&str] = &["exchange.blueprints.read", "exchange.blueprints.write"];
pub const SCOPES_STOCK: &[&str] = &["exchange.stock.read", "exchange.stock.write"];
pub const SCOPES_HANGAR: &[&str] = &["exchange.hangar.read", "exchange.hangar.write"];
pub const SCOPES_DEMAND: &[&str] = &["exchange.demand.read"];
/// ⚠ A right of its own next to the hangar, not in `SCOPES_HANGAR`: else the
/// hangar sync would stop for everyone who has not granted it. Without it
/// there is only no unit choice.
pub const SCOPES_ORGUNITS: &[&str] = &["exchange.orgunits.read"];
/// Released by the Basetool for Verse-Kit.
pub const REQUEST_ORGUNITS: bool = true;
/// Orders one by one in the demand, voluntary.
pub const SCOPES_DEMAND_DETAILS: &[&str] = &["exchange.demand.details"];
/// The unit storage: reading is the area; entries per owner and booking
/// are voluntary rights on top.
pub const SCOPES_ORGSTOCK: &[&str] = &["exchange.orgstock.read"];
pub const SCOPES_ORGSTOCK_DETAILS: &[&str] = &["exchange.orgstock.details"];
pub const SCOPES_ORGSTOCK_WRITE: &[&str] = &["exchange.orgstock.write"];
/// The own refinery orders: reading and writing. ⚠ `exchange.refinery.store` (booking the yield at the pickup) is
/// never asked for: the stock is booked by the stock sync alone, a pickup only closes the order.
pub const SCOPES_REFINERY: &[&str] = &["exchange.refinery.read", "exchange.refinery.write"];
/// The Kartellbank: reading is the area; raising, correcting and cancelling own requests is a voluntary right on
/// top, the queue of a bank employee a voluntary right of its own switch.
pub const SCOPES_BANK: &[&str] = &["exchange.bank.read"];
pub const SCOPES_BANK_REQUEST: &[&str] = &["exchange.bank.request"];
pub const SCOPES_BANK_QUEUE: &[&str] = &["exchange.bank.queue"];

/// Switches in the settings.
pub const SETTING_BLUEPRINTS: &str = "basetool_bauplaene";
pub const SETTING_STOCK: &str = "basetool_lager";
pub const SETTING_SHIPS: &str = "basetool_hangar";
pub const SETTING_DEMAND: &str = "basetool_bedarf";
pub const SETTING_ORGSTOCK: &str = "basetool_staffellager";
pub const SETTING_REFINERY: &str = "basetool_raffinerie";
/// The unit new refinery orders belong to: empty means the only unit of the member, if there is one.
pub const SETTING_REFINERY_UNIT: &str = "basetool_raffinerie_einheit";
pub const SETTING_BANK: &str = "basetool_bank";
/// The queue of a bank employee, only together with the bank.
pub const SETTING_BANK_QUEUE: &str = "basetool_bank_warteschlange";
/// Booking in the unit storage as logistician, only together with reading.
pub const SETTING_ORGSTOCK_WRITE: &str = "basetool_staffellager_buchen";
/// Name of this installation in the Basetool.
pub const SETTING_LABEL: &str = "basetool_name";
/// Unit for new ships: empty means the Basetool decides.
pub const SETTING_NEW_SHIP_UNIT: &str = "basetool_einheit_neu";

/// The send routes the Basetool can hold back with a mass change lock.
pub const ROUTE_BLUEPRINTS: &str = "/me/blueprints/changes";
pub const ROUTE_STOCK: &str = "/me/stock/changes";
pub const ROUTE_SHIPS: &str = "/me/ships/changes";
pub const ROUTE_REFINERY: &str = "/me/refinery-orders/changes";
pub const SEND_ROUTES: [&str; 4] = [ROUTE_BLUEPRINTS, ROUTE_STOCK, ROUTE_SHIPS, ROUTE_REFINERY];

/// The areas in their order: switch and the rights it needs.
pub const AREAS: [(&str, &[&str]); 7] = [
    (SETTING_BLUEPRINTS, SCOPES_BLUEPRINTS),
    (SETTING_STOCK, SCOPES_STOCK),
    (SETTING_SHIPS, SCOPES_HANGAR),
    (SETTING_DEMAND, SCOPES_DEMAND),
    (SETTING_ORGSTOCK, SCOPES_ORGSTOCK),
    (SETTING_REFINERY, SCOPES_REFINERY),
    (SETTING_BANK, SCOPES_BANK),
];

/// Reads the switches; the settings file in the program.
pub trait Switches {
    fn on(&self, setting: &str) -> bool;
}

impl Switches for crate::paths::Paths {
    fn on(&self, setting: &str) -> bool {
        self.setting_bool(setting, false)
    }
}

impl<F: Fn(&str) -> bool> Switches for F {
    fn on(&self, setting: &str) -> bool {
        self(setting)
    }
}

fn add(out: &mut Vec<String>, scopes: &[&str]) {
    for scope in scopes {
        if !out.iter().any(|s| s == scope) {
            out.push((*scope).to_string());
        }
    }
}

/// Whether any area is switched on.
pub fn enabled(switches: &dyn Switches) -> bool {
    AREAS.iter().any(|(setting, _)| switches.on(setting))
}

/// Whether a switched-on area needs the list of the member's units.
pub fn units_wanted(switches: &dyn Switches) -> bool {
    switches.on(SETTING_SHIPS) || switches.on(SETTING_STOCK) || switches.on(SETTING_REFINERY)
}

/// The voluntary rights of switched-on areas: without them the area runs,
/// only without the extra (orders one by one, entries per owner, booking).
pub fn optional_scopes(switches: &dyn Switches) -> Vec<&'static [&'static str]> {
    let mut out = Vec::new();
    if switches.on(SETTING_DEMAND) {
        out.push(SCOPES_DEMAND_DETAILS);
    }
    if switches.on(SETTING_ORGSTOCK) {
        out.push(SCOPES_ORGSTOCK_DETAILS);
        if switches.on(SETTING_ORGSTOCK_WRITE) {
            out.push(SCOPES_ORGSTOCK_WRITE);
        }
    }
    if switches.on(SETTING_BANK) {
        out.push(SCOPES_BANK_REQUEST);
        if switches.on(SETTING_BANK_QUEUE) {
            out.push(SCOPES_BANK_QUEUE);
        }
    }
    out
}

/// The rights of all switched-on areas; only these are asked for.
pub fn wanted_scopes(switches: &dyn Switches) -> Vec<String> {
    let mut out = Vec::new();
    for (setting, scopes) in AREAS {
        if switches.on(setting) {
            add(&mut out, scopes);
        }
    }
    // The unit list for hangar, stock (sharing into a unit's pool) and refinery (the owner of a new order).
    if units_wanted(switches) && REQUEST_ORGUNITS {
        add(&mut out, SCOPES_ORGUNITS);
    }
    for scopes in optional_scopes(switches) {
        add(&mut out, scopes);
    }
    out
}

/// Voluntary rights switched on but not granted.
pub fn missing_optional(switches: &dyn Switches, granted: &[String]) -> Vec<String> {
    optional_scopes(switches)
        .into_iter()
        .flatten()
        .filter(|s| !granted.iter().any(|g| g == *s))
        .map(|s| (*s).to_string())
        .collect()
}

/// Whether all of `scopes` are in `have`.
pub fn all_in(scopes: &[&str], have: &[String]) -> bool {
    scopes.iter().all(|s| have.iter().any(|h| h == s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_switched_on_areas_ask() {
        let none = |_: &str| false;
        assert!(wanted_scopes(&none).is_empty());
        assert!(!enabled(&none));
        let orgstock = |s: &str| s == SETTING_ORGSTOCK || s == SETTING_ORGSTOCK_WRITE;
        assert_eq!(
            wanted_scopes(&orgstock),
            ["exchange.orgstock.read", "exchange.orgstock.details", "exchange.orgstock.write"]
        );
        // Booking without reading asks for nothing.
        let write_only = |s: &str| s == SETTING_ORGSTOCK_WRITE;
        assert!(wanted_scopes(&write_only).is_empty());
        let stock = |s: &str| s == SETTING_STOCK;
        assert_eq!(wanted_scopes(&stock), ["exchange.stock.read", "exchange.stock.write", "exchange.orgunits.read"]);
        let refinery = |s: &str| s == SETTING_REFINERY;
        assert_eq!(
            wanted_scopes(&refinery),
            ["exchange.refinery.read", "exchange.refinery.write", "exchange.orgunits.read"],
            "never the store right"
        );
        let bank = |s: &str| s == SETTING_BANK;
        assert_eq!(wanted_scopes(&bank), ["exchange.bank.read", "exchange.bank.request"], "the queue only on request");
        let staff = |s: &str| s == SETTING_BANK || s == SETTING_BANK_QUEUE;
        assert_eq!(wanted_scopes(&staff), ["exchange.bank.read", "exchange.bank.request", "exchange.bank.queue"]);
        let queue_only = |s: &str| s == SETTING_BANK_QUEUE;
        assert!(wanted_scopes(&queue_only).is_empty(), "the queue goes with the bank");
        let granted = vec!["exchange.orgstock.details".to_string()];
        assert_eq!(missing_optional(&orgstock, &granted), ["exchange.orgstock.write"]);
    }
}
