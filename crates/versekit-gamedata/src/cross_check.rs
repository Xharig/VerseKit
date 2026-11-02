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

//! The standing cross-check of the own game values against scmdb's at the
//! same build (decided 22.10.: scmdb stays for good, as fallback and as
//! cross-check). Compared are the recipes with their slots and modifiers,
//! and of every product its kind, name, maker, size, grade, integrity,
//! shield values, fire modes and ammunition. Cooling, power, quantum, radar,
//! tractor, salvage and armour values are not read from the `Data.p4k` yet
//! and not compared.
//!
//! Expected differences have a line of their own and count as equal: a
//! slot without a name of its own (scmdb shows the placeholder text) and
//! the integrity the own values add for weapons and other items.

use std::collections::BTreeMap;

use crate::values::{GameValues, ItemType, ItemValues, Recipe, Stat};

/// How often one field was equal and how often different, with the first
/// differences.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldCount {
    /// Comparisons with the same value.
    pub equal: usize,
    /// Comparisons with different values.
    pub different: usize,
    /// The first differences: place, own value, scmdb's value.
    pub examples: Vec<String>,
}

/// The outcome of comparing the own values with scmdb's at the same build.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CrossCheck {
    /// Per field (a German label, e.g. `Modus: Feuerrate`) its counts.
    pub fields: BTreeMap<&'static str, FieldCount>,
    /// The tags of recipes only scmdb has.
    pub only_scmdb: Vec<String>,
    /// The tags of recipes only the own values have.
    pub only_own: Vec<String>,
    /// How many products were compared.
    pub products: usize,
}

impl CrossCheck {
    fn check(&mut self, field: &'static str, place: &str, own: &str, theirs: &str) {
        let entry = self.fields.entry(field).or_default();
        if own == theirs {
            entry.equal += 1;
        } else {
            entry.different += 1;
            if entry.examples.len() < 4 {
                entry.examples.push(format!("{place}: eigen={own} scmdb={theirs}"));
            }
        }
    }

    fn number(&mut self, field: &'static str, place: &str, own: Option<f64>, theirs: Option<f64>) {
        let same = match (own, theirs) {
            (Some(a), Some(b)) => (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1.0),
            (None, None) => return,
            _ => false,
        };
        let text = |v: Option<f64>| v.map_or("—".to_string(), |v| format!("{v}"));
        if same {
            self.check(field, place, "", "");
        } else {
            self.check(field, place, &text(own), &text(theirs));
        }
    }

    /// The differences of all fields, and the recipes only one side has.
    pub fn differences(&self) -> usize {
        self.fields.values().map(|count| count.different).sum::<usize>() + self.only_scmdb.len() + self.only_own.len()
    }

    /// The fields with differences and their first example, for a note in
    /// the error log.
    pub fn summary(&self) -> String {
        self.fields
            .iter()
            .filter(|(_, count)| count.different > 0)
            .map(|(field, count)| {
                format!("{field}: {} ({})", count.different, count.examples.first().map_or("", String::as_str))
            })
            .chain((!self.only_scmdb.is_empty()).then(|| format!("nur bei scmdb: {}", self.only_scmdb.len())))
            .chain((!self.only_own.is_empty()).then(|| format!("nur eigen: {}", self.only_own.len())))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// The product scmdb's record names, before scmdb puts the meant one in
/// its place.
fn raw_product(recipe: &Recipe) -> Option<&str> {
    match recipe.corrected.iter().find(|mark| mark.field == "product") {
        Some(mark) => Some(mark.raw.as_str()).filter(|raw| !raw.is_empty()),
        None => recipe.product.as_deref(),
    }
}

fn recipe(tally: &mut CrossCheck, own: &Recipe, theirs: &Recipe) {
    let place = own.tag.as_str();
    tally.check("Rezept: Produkt", place, own.product.as_deref().unwrap_or("—"), raw_product(theirs).unwrap_or("—"));
    tally.check("Rezept: Stufen", place, &own.tiers.len().to_string(), &theirs.tiers.len().to_string());
    for (a, b) in own.tiers.iter().zip(&theirs.tiers) {
        tally.number(
            "Rezept: Herstellzeit",
            place,
            a.craft_time_seconds.map(|v| v as f64),
            b.craft_time_seconds.map(|v| v as f64),
        );
        tally.check("Rezept: Plätze", place, &a.slots.len().to_string(), &b.slots.len().to_string());
        for (x, y) in a.slots.iter().zip(&b.slots) {
            let at = format!("{place} › {}", y.name);
            // scmdb shows the placeholder text where a slot has no name; the
            // own values name it after its debug name.
            if y.name == "<= PLACEHOLDER =>" {
                tally.check("Platz: Name (Platzhalter, bewusst nach Debugname)", &at, "", "");
            } else {
                tally.check("Platz: Name", &at, &x.name, &y.name);
            }
            tally.check("Platz: Material", &at, x.material(), y.material());
            let first = |slot: &crate::values::Slot| slot.options.first().cloned().unwrap_or_default();
            let (ox, oy) = (first(x), first(y));
            tally.check("Platz: Art", &at, &format!("{:?}", ox.kind), &format!("{:?}", oy.kind));
            tally.number("Platz: Menge", &at, ox.quantity, oy.quantity);
            tally.number(
                "Platz: Mindestqualität",
                &at,
                ox.min_quality.map(|v| v as f64),
                oy.min_quality.map(|v| v as f64),
            );
            let ranges = |slot: &crate::values::Slot| {
                slot.all_modifiers()
                    .map(|m| {
                        format!(
                            "{} {:?} {}–{} ×{}–{}",
                            m.property_key, m.kind, m.start_quality, m.end_quality, m.at_start, m.at_end
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            tally.check("Platz: Modifikatoren", &at, &ranges(x), &ranges(y));
            let names = |slot: &crate::values::Slot| {
                slot.all_modifiers().filter_map(|m| m.property_name.clone()).collect::<Vec<_>>().join("; ")
            };
            tally.check("Platz: Eigenschaftsnamen", &at, &names(x), &names(y));
        }
    }
}

fn item(tally: &mut CrossCheck, place: &str, own: &ItemValues, theirs: &ItemValues) {
    tally.check("Gegenstand: Art", place, &format!("{:?}", own.item_type), &format!("{:?}", theirs.item_type));
    tally.check(
        "Gegenstand: Anbauart",
        place,
        own.attach_type.as_deref().unwrap_or("—"),
        theirs.attach_type.as_deref().unwrap_or("—"),
    );
    tally.check("Gegenstand: Name", place, own.name.as_deref().unwrap_or("—"), theirs.name.as_deref().unwrap_or("—"));
    tally.check(
        "Gegenstand: Hersteller",
        place,
        own.manufacturer.as_deref().unwrap_or("—"),
        theirs.manufacturer.as_deref().unwrap_or("—"),
    );
    tally.number("Gegenstand: Größe", place, own.size.map(|v| v as f64), theirs.size.map(|v| v as f64));
    tally.number("Gegenstand: Grad", place, own.grade.map(|v| v as f64), theirs.grade.map(|v| v as f64));
    for stat in [Stat::Health, Stat::ShieldHealth, Stat::ShieldRegen, Stat::DownedRegenDelay, Stat::DamagedRegenDelay] {
        // scmdb gives no integrity for weapons and other items; the own
        // values add it. For components and armour it must be the same.
        if theirs.stat(stat).is_none() && stat == Stat::Health {
            match own.item_type {
                ItemType::Weapon => {
                    tally.check("Wert: Integrität zusätzlich bei Waffen (scmdb ohne)", place, "", "");
                    continue;
                }
                ItemType::Other => {
                    tally.check("Wert: Integrität zusätzlich bei Sonstigem (scmdb ohne)", place, "", "");
                    continue;
                }
                ItemType::ShipComponent | ItemType::Armor => {}
            }
        }
        let field = match stat {
            Stat::Health => "Wert: Integrität",
            Stat::ShieldHealth => "Wert: Schildstärke",
            Stat::ShieldRegen => "Wert: Schildregeneration",
            Stat::DownedRegenDelay => "Wert: Pause nach Zusammenbruch",
            _ => "Wert: Pause nach Treffer",
        };
        tally.number(field, place, own.stat(stat), theirs.stat(stat));
    }
    let (Some(a), Some(b)) = (own.weapon.as_ref(), theirs.weapon.as_ref()) else {
        tally.check("Waffe: vorhanden", place, &own.weapon.is_some().to_string(), &theirs.weapon.is_some().to_string());
        return;
    };
    tally.check("Waffe: Feuermodi", place, &a.fire_modes.len().to_string(), &b.fire_modes.len().to_string());
    for (x, y) in a.fire_modes.iter().zip(&b.fire_modes) {
        let at = format!("{place} › {}", y.action.name.as_deref().unwrap_or("?"));
        tally.check("Modus: Art", &at, &x.action.kind, &y.action.kind);
        tally.check("Modus: Folge", &at, &x.sequence.len().to_string(), &y.sequence.len().to_string());
        let (sx, sy) = (x.shown(), y.shown());
        tally.number("Modus: Feuerrate", &at, sx.fire_rate, sy.fire_rate);
        tally.number("Modus: Verzögerung", &at, sx.delay, sy.delay);
        tally.number("Modus: Schadensfaktor", &at, sx.damage_multiplier, sy.damage_multiplier);
        tally.number("Modus: Geschosse", &at, sx.pellet_count.map(|v| v as f64), sy.pellet_count.map(|v| v as f64));
        tally.number("Modus: Hitze", &at, sx.heat_per_shot, sy.heat_per_shot);
        tally.number("Modus: Streuung min", &at, sx.spread.map(|s| s.0), sy.spread.map(|s| s.0));
        tally.number("Modus: Streuung max", &at, sx.spread.map(|s| s.1), sy.spread.map(|s| s.1));
        let recoil = |r: Option<crate::values::Recoil>| r.map(|r| (r.yaw_max_deg, r.pitch_max_deg, r.smooth_time));
        tally.check("Modus: Rückstoß", &at, &format!("{:?}", recoil(sx.recoil)), &format!("{:?}", recoil(sy.recoil)));
    }
    let (ammo_a, ammo_b) = (a.ammo.clone().unwrap_or_default(), b.ammo.clone().unwrap_or_default());
    tally.number("Munition: Geschwindigkeit", place, ammo_a.speed, ammo_b.speed);
    tally.number("Munition: Lebensdauer", place, ammo_a.lifetime, ammo_b.lifetime);
    tally.check(
        "Munition: Schaden",
        place,
        &format!("{:?}", ammo_a.damage.map(|d| d.0)),
        &format!("{:?}", ammo_b.damage.map(|d| d.0)),
    );
    tally.check(
        "Munition: Detonation",
        place,
        &format!("{:?}", ammo_a.detonation.map(|d| d.0)),
        &format!("{:?}", ammo_b.detonation.map(|d| d.0)),
    );
    tally.number("Waffe: Magazin", place, a.magazine.map(|v| v as f64), b.magazine.map(|v| v as f64));
}

/// Compares the own values with scmdb's of the same build: every recipe
/// scmdb knows, its slots and modifiers, and the product's kind, name,
/// maker, size, grade, integrity, shield values, fire modes and ammunition.
pub fn compare(own: &GameValues, scmdb: &GameValues) -> CrossCheck {
    let mut tally = CrossCheck::default();
    for theirs_recipe in &scmdb.recipes {
        let Some(own_recipe) = own.recipes.iter().find(|r| r.guid == theirs_recipe.guid) else {
            tally.only_scmdb.push(theirs_recipe.tag.clone());
            continue;
        };
        recipe(&mut tally, own_recipe, theirs_recipe);
        let own_item = theirs_recipe.product.as_deref().and_then(|g| own.items.get(g));
        let (a, b) = match (own_item, scmdb.product(theirs_recipe)) {
            (Some(a), Some(b)) => (a, b),
            (None, None) => continue,
            (a, b) => {
                let side = |found: bool| if found { "da" } else { "fehlt" };
                tally.check("Produkt: auf beiden Seiten", &theirs_recipe.tag, side(a.is_some()), side(b.is_some()));
                continue;
            }
        };
        tally.products += 1;
        item(&mut tally, &theirs_recipe.tag, a, b);
    }
    tally.only_own =
        own.recipes.iter().filter(|r| !scmdb.recipes.iter().any(|t| t.guid == r.guid)).map(|r| r.tag.clone()).collect();
    tally
}
