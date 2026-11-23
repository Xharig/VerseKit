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

//! The mining equipment of the game data and the player's choice of it: laser heads, modules and gadgets with
//! the values the answer of [`crate::rock_crack`] needs, and the ships with their mining heads.
//!
//! The equipment file ([`FILE`]) is written by `versekit-gamedata` beside the slots of the ships when the game
//! data are read; the ships with their mining slots come from the slots file of the game data
//! ([`crate::erkul_slots::GAME_SHIPS_FILE`]). The choice lives in the settings ([`keys`]).
//!
//! The changes of resistance and instability stand in percent as the game stores them (`-25` lowers by a
//! quarter); the power of a module is a factor (`1.35`).

use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::rock_crack::{Factors, Laser, Loadout};

/// The equipment of the game data, in the data folder beside the slots of the ships.
pub const FILE: &str = "spiel-bergbau.json";
/// The format of [`FILE`] this version reads and writes.
pub const FORMAT: i64 = 1;

/// The settings of the choice.
pub mod keys {
    /// The mining ship of the hangar by its key (`name\u{1f}maker\u{1f}0`).
    pub const SHIP: &str = "stein_schiff";
    /// The gadget on the rock, by its ref; empty for none.
    pub const GADGET: &str = "stein_gadget";
    /// The active module counted as in use, by its ref; empty for none.
    pub const ACTIVE: &str = "stein_aktiv";
    /// The line of the answer above the signature and in the overlay.
    pub const SHOW_VERDICT: &str = "stein_zeile_aufbrechbar";
    /// The line of the worth.
    pub const SHOW_WORTH: &str = "stein_zeile_wert";
}

/// The further changes of a head, module or gadget in percent as the game stores them; `None` where the game data
/// do not set it. They do not enter the answer; the page shows them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct More {
    /// Size of the optimal charge window.
    pub window: Option<f64>,
    /// Rate of the optimal charge window.
    pub rate: Option<f64>,
    /// Rate of the catastrophic (overcharge) window.
    pub overcharge: Option<f64>,
    pub shatter: Option<f64>,
    pub cluster: Option<f64>,
    /// The share of inert material filtered out.
    pub filter: Option<f64>,
}

impl More {
    fn read(entry: &Value) -> More {
        More {
            window: number(entry.get("fenster")),
            rate: number(entry.get("laderate")),
            overcharge: number(entry.get("ueberladung")),
            shatter: number(entry.get("splitter")),
            cluster: number(entry.get("clustering")),
            filter: number(entry.get("filter")),
        }
    }
}

/// A laser head.
#[derive(Clone, Debug, PartialEq)]
pub struct Head {
    pub reference: String,
    pub name: String,
    pub size: i64,
    /// The power of its cracking beam.
    pub power: f64,
    /// The lowest throttle as a share of the power (0–1), where the game data set it.
    pub throttle_min: Option<f64>,
    /// The power of its extraction beam.
    pub extraction: Option<f64>,
    /// Up to this distance the beam is full, from this one it does nothing (m).
    pub range_full: Option<f64>,
    pub range_zero: Option<f64>,
    /// Change of resistance and instability in percent.
    pub resistance: f64,
    pub instability: f64,
    pub more: More,
    /// Slots for modules.
    pub slots: usize,
}

/// A module for a head.
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub reference: String,
    pub name: String,
    /// Used on demand with charges (else always on).
    pub active: bool,
    pub charges: Option<i64>,
    /// Seconds one use lasts.
    pub lifetime: Option<f64>,
    /// Factor of the power.
    pub power: f64,
    pub resistance: f64,
    pub instability: f64,
    pub more: More,
}

/// A gadget placed on the rock.
#[derive(Clone, Debug, PartialEq)]
pub struct Gadget {
    pub reference: String,
    pub name: String,
    pub resistance: f64,
    pub instability: f64,
    pub more: More,
}

/// A ship with mining slots.
#[derive(Clone, Debug, PartialEq)]
pub struct Ship {
    pub id: String,
    pub name: String,
    /// One entry per mining head: its size and the head it has at the factory.
    pub heads: Vec<(i64, Option<String>)>,
}

/// Everything of the game data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gear {
    pub heads: Vec<Head>,
    pub modules: Vec<Module>,
    pub gadgets: Vec<Gadget>,
    pub ships: Vec<Ship>,
    /// The game build the file was read from.
    pub build: String,
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn list<'a>(data: &'a Value, key: &str) -> &'a [Value] {
    match data.get(key) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

impl Gear {
    /// Reads the equipment file and the ships of the slots file of `paths`.
    pub fn load(paths: &Paths) -> Gear {
        let equipment = jsonio::read_json(&paths.app_file(FILE)).ok();
        let ships = jsonio::read_json(&paths.app_file(crate::erkul_slots::GAME_SHIPS_FILE)).ok();
        Gear::from_values(equipment.as_ref(), ships.as_ref())
    }

    /// The equipment of a file already read (of format [`FORMAT`], else none) and the ships of a slots file.
    pub fn from_values(equipment: Option<&Value>, ships: Option<&Value>) -> Gear {
        let mut gear = Gear::default();
        if let Some(data) = equipment.filter(|d| number(d.get("format")) == Some(FORMAT as f64)) {
            gear.build = text(data.get("spielversion"));
            for head in list(data, "koepfe") {
                let Some(power) = number(head.get("leistung")).filter(|p| *p > 0.0) else { continue };
                gear.heads.push(Head {
                    reference: text(head.get("ref")),
                    name: text(head.get("name")),
                    size: number(head.get("groesse")).unwrap_or(0.0) as i64,
                    power,
                    throttle_min: number(head.get("drossel_min")),
                    extraction: number(head.get("extraktion")),
                    range_full: number(head.get("reichweite_voll")),
                    range_zero: number(head.get("reichweite_null")),
                    resistance: number(head.get("widerstand")).unwrap_or(0.0),
                    instability: number(head.get("instabilitaet")).unwrap_or(0.0),
                    more: More::read(head),
                    slots: number(head.get("plaetze")).unwrap_or(0.0).max(0.0) as usize,
                });
            }
            for module in list(data, "module") {
                gear.modules.push(Module {
                    reference: text(module.get("ref")),
                    name: text(module.get("name")),
                    active: matches!(module.get("aktiv"), Some(Value::Bool(true))),
                    charges: number(module.get("ladungen")).map(|n| n as i64),
                    lifetime: number(module.get("dauer")),
                    power: number(module.get("leistung")).unwrap_or(1.0),
                    resistance: number(module.get("widerstand")).unwrap_or(0.0),
                    instability: number(module.get("instabilitaet")).unwrap_or(0.0),
                    more: More::read(module),
                });
            }
            for gadget in list(data, "gadgets") {
                // A file written before the game data's name was required holds entries named by their class.
                let (name, class) = (text(gadget.get("name")), text(gadget.get("klasse")));
                if name.trim().is_empty() || (!class.is_empty() && name == class) {
                    continue;
                }
                gear.gadgets.push(Gadget {
                    reference: text(gadget.get("ref")),
                    name: text(gadget.get("name")),
                    resistance: number(gadget.get("widerstand")).unwrap_or(0.0),
                    instability: number(gadget.get("instabilitaet")).unwrap_or(0.0),
                    more: More::read(gadget),
                });
            }
            for items in [&mut gear.heads as &mut dyn SortByName, &mut gear.modules, &mut gear.gadgets] {
                items.sort_by_name();
            }
        }
        if let Some(Value::Object(all)) = ships.and_then(|s| s.get("schiffe")) {
            for (_, ship) in all {
                let heads: Vec<(i64, Option<String>)> = list(ship, "slots")
                    .iter()
                    .filter(|slot| text(slot.get("art")) == "WeaponMining")
                    .map(|slot| {
                        let stock = slot.get("werk").map(|w| text(w.get("ref"))).filter(|r| !r.is_empty());
                        (number(slot.get("groesse")).unwrap_or(0.0) as i64, stock)
                    })
                    .collect();
                if !heads.is_empty() {
                    gear.ships.push(Ship { id: text(ship.get("id")), name: text(ship.get("name")), heads });
                }
            }
            gear.ships.sort_by_key(|ship| ship.name.to_lowercase());
        }
        gear
    }

    pub fn is_empty(&self) -> bool {
        self.heads.is_empty()
    }

    pub fn head(&self, reference: &str) -> Option<&Head> {
        self.heads.iter().find(|h| h.reference == reference)
    }

    pub fn module(&self, reference: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.reference == reference)
    }

    pub fn gadget(&self, reference: &str) -> Option<&Gadget> {
        self.gadgets.iter().find(|g| g.reference == reference)
    }

    pub fn ship(&self, id: &str) -> Option<&Ship> {
        self.ships.iter().find(|s| s.id == id)
    }
}

trait SortByName {
    fn sort_by_name(&mut self);
}

macro_rules! sort_by_name {
    ($($kind:ty),*) => {$(
        impl SortByName for Vec<$kind> {
            fn sort_by_name(&mut self) {
                self.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
        }
    )*};
}
sort_by_name!(Head, Module, Gadget);

/// One head of the choice: the head and the modules in it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chosen {
    pub head: String,
    pub modules: Vec<String>,
}

/// The part in a slot: the one of the plan of the hangar where it names one, else the factory part; `None` for a
/// slot the plan left empty or that has none.
fn part_in(slot: &crate::erkul_slots::SlotInfo, plan: &crate::jsonio::Map) -> Option<String> {
    let planned = crate::hangar::get(plan, &slot.path).and_then(Value::as_object);
    if !slot.fixed && planned.is_some_and(crate::cart::is_emptied) {
        return None;
    }
    let reference = planned.map(|p| crate::hangar::text(p, "ref")).filter(|r| !r.is_empty());
    reference.map(str::to_string).or_else(|| slot.factory.as_ref().map(|(r, _)| r.clone()))
}

/// The heads and modules fitted: the mining slots of a ship (the plan applied, as `erkul_slots::hardpoints` gives
/// them), in order, each head with the modules in the slots below it.
pub fn fitted(slots: &[crate::erkul_slots::SlotInfo], plan: &crate::jsonio::Map) -> Vec<Chosen> {
    slots
        .iter()
        .filter(|slot| slot.kind == "WeaponMining")
        .map(|head| {
            let below = format!("{}/", head.path);
            let modules = slots
                .iter()
                .filter(|slot| slot.kind == "MiningModifier" && slot.path.starts_with(&below))
                .filter_map(|slot| part_in(slot, plan))
                .collect();
            Chosen { head: part_in(head, plan).unwrap_or_default(), modules }
        })
        .collect()
}

/// A mining ship of the hangar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HangarShip {
    /// Name and maker as the hangar keeps them, and how many alike stand before it: `name\u{1f}maker\u{1f}0`.
    pub key: String,
    /// Its place among the ships of the hangar.
    pub index: usize,
    pub name: String,
    pub lasers: Vec<Chosen>,
}

/// The ships of the hangar `data` with at least one mining head, with what they have fitted.
pub fn hangar_ships(data: &crate::jsonio::Map, slots: &crate::erkul_slots::ErkulData) -> Vec<HangarShip> {
    let ships = crate::hangar::ships(data);
    let mut out = Vec::new();
    for (i, entry) in ships.iter().enumerate() {
        let field = |key: &str| crate::hangar::field_text(entry, key);
        let id = crate::ships::ShipId {
            name: field("name"),
            maker: field("hersteller"),
            short: field("kurz"),
            maker_short: field("hkurz"),
        };
        let plan = match crate::hangar::get(entry, "belegung") {
            Some(Value::Object(plan)) => plan.clone(),
            _ => crate::jsonio::Map::new(),
        };
        let lasers = fitted(&crate::erkul_slots::hardpoints(&slots.ships, &slots.parts, id, &plan), &plan);
        if lasers.is_empty() {
            continue;
        }
        let before = ships[..i]
            .iter()
            .filter(|s| {
                crate::hangar::field_text(s, "name") == id.name && crate::hangar::field_text(s, "hersteller") == id.maker
            })
            .count();
        out.push(HangarShip {
            key: format!("{}\u{1f}{}\u{1f}{before}", id.name, id.maker),
            index: i,
            name: id.name.to_string(),
            lasers,
        });
    }
    out
}

/// The mining ships of the player's hangar.
pub fn hangar_ships_of(paths: &Paths) -> Vec<HangarShip> {
    let data = crate::hangar::load(&paths.app_file(crate::hangar::FILE));
    hangar_ships(&data, &crate::erkul_slots::ErkulData::load(paths))
}

/// The player's choice: the mining ship of the hangar the answer is computed for and what it has fitted (from the
/// hangar, never stored here), the gadget on the rock and the active module in use (in the settings).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    /// The key of the ship in the hangar ([`HangarShip::key`]).
    pub ship: String,
    pub lasers: Vec<Chosen>,
    pub gadget: String,
    pub active: String,
}

impl Choice {
    /// The choice of the settings with the fitting of the chosen ship of the hangar (the first mining ship where
    /// none is chosen or the chosen one is gone).
    pub fn read(paths: &Paths) -> Choice {
        Choice::read_with(paths, &hangar_ships_of(paths))
    }

    pub fn read_with(paths: &Paths, ships: &[HangarShip]) -> Choice {
        let settings = paths.settings();
        let get = |key: &str| settings.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        Choice::from_settings(get(keys::SHIP), get(keys::GADGET), get(keys::ACTIVE)).with_ship(ships)
    }

    pub fn from_settings(ship: Option<Value>, gadget: Option<Value>, active: Option<Value>) -> Choice {
        Choice { ship: text(ship.as_ref()), lasers: Vec::new(), gadget: text(gadget.as_ref()), active: text(active.as_ref()) }
    }

    /// The fitting of the chosen ship among `ships`, the first one where the choice names none of them.
    pub fn with_ship(mut self, ships: &[HangarShip]) -> Choice {
        let ship = ships.iter().find(|s| s.key == self.ship).or_else(|| ships.first());
        self.ship = ship.map(|s| s.key.clone()).unwrap_or_default();
        self.lasers = ship.map(|s| s.lasers.clone()).unwrap_or_default();
        self
    }

    /// The settings that keep the choice (the fitting stays in the hangar).
    pub fn setting_values(&self) -> Vec<(&'static str, Value)> {
        vec![
            (keys::SHIP, Value::str(self.ship.clone())),
            (keys::GADGET, Value::str(self.gadget.clone())),
            (keys::ACTIVE, Value::str(self.active.clone())),
        ]
    }

    /// The choice in numbers: heads the data do not know are left out, as are unknown modules.
    pub fn loadout(&self, gear: &Gear) -> Loadout {
        let lasers = self
            .lasers
            .iter()
            .filter_map(|chosen| {
                let head = gear.head(&chosen.head)?;
                let modules = chosen
                    .modules
                    .iter()
                    .filter_map(|m| gear.module(m))
                    .filter(|m| !m.active)
                    .take(head.slots)
                    .map(|m| Factors { power: m.power, ..Factors::from_percent(0.0, m.resistance, m.instability) })
                    .collect();
                Some(Laser {
                    power: head.power,
                    min_share: head.throttle_min.unwrap_or(0.0),
                    head: Factors::from_percent(0.0, head.resistance, head.instability),
                    modules,
                })
            })
            .collect();
        let gadget = gear.gadget(&self.gadget).map(|g| Factors::from_percent(0.0, g.resistance, g.instability));
        let active = gear
            .module(&self.active)
            .map(|m| Factors { power: m.power, ..Factors::from_percent(0.0, m.resistance, m.instability) });
        Loadout { lasers, gadget, active }
    }
}

#[cfg(test)]
mod tests;
