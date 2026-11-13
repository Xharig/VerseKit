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

//! What the quality of the materials makes of an item: the value at quality
//! `q`.
//!
//! The computation follows scmdb.net exactly, so both tools show the same
//! numbers (port of `product_stats.py` and `crafting.py` of
//! 4.12):
//!
//! * Each slot has the quality of its material, 0 to 1000; 500 is the base
//!   value. Per property the range the quality falls into counts, else the
//!   last one; within it the factor is linear.
//! * Several slots on the same property are **added**, not multiplied:
//!   ×1.042 and ×1.032 give ×1.074.
//! * Additive modifiers (power pips) are whole numbers, rounded and summed.
//!
//! One addition over 4.12: a weapon's integrity (`health_maxhealth`) is
//! scaled as well. The scmdb item files hold no integrity for weapons, so
//! 4.12 never showed it; the `Data.p4k` does.

use std::collections::BTreeMap;

use crate::values::{Ammo, Damage, FireAction, ItemType, ItemValues, Modifier, ModifierKind, Recipe, Slot, Stat};

/// The quality a slot has when nothing else is known: the base value.
pub const BASE_QUALITY: f64 = 500.0;

/// The property key of each component stat quality scales (scmdb `Bx`).
const COMPONENT_KEYS: [(&str, Stat); 14] = [
    ("health_maxhealth", Stat::Health),
    ("shield_maxhealth", Stat::ShieldHealth),
    ("itemresource_coolantgeneration", Stat::CoolingRate),
    ("quantum_fuelrequirement", Stat::QuantumFuelRequirement),
    ("quantum_speed", Stat::DriveSpeed),
    ("radar_minaimassistdistance", Stat::MinAimAssistRange),
    ("radar_maxaimassistdistance", Stat::MaxAimAssistRange),
    ("weapon_hullscraping_speed", Stat::SalvageSpeedMultiplier),
    ("weapon_hullscraping_radius", Stat::RadiusMultiplier),
    ("weapon_hullscraping_efficiency", Stat::ExtractionEfficiency),
    ("weapon_tractor_force", Stat::MaxForce),
    ("weapon_tractor_fullstrengthdist", Stat::FullStrengthDistance),
    ("weapon_tractor_maxdist", Stat::MaxDistance),
    ("weapon_tractor_maxvolume", Stat::MaxVolume),
];

/// The combined effect of all slots: a factor per property and the pips per
/// additive property.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Factors {
    /// The factor per property key; a property without modifier is absent
    /// (factor 1).
    pub mul: BTreeMap<String, f64>,
    /// The pips per additive property key.
    pub pips: BTreeMap<String, i64>,
}

impl Factors {
    /// The factor of `key`, `None` when no slot changes it.
    pub fn factor(&self, key: &str) -> Option<f64> {
        self.mul.get(key).copied()
    }
}

/// An item before and after the quality of its materials.
#[derive(Debug, Clone, PartialEq)]
pub struct Crafted {
    /// The item as the game data describe it (quality 500 everywhere).
    pub base: ItemValues,
    /// The item at the given qualities.
    pub made: ItemValues,
    /// The combined effect of the slots.
    pub factors: Factors,
}

/// Python's truth test of an optional number: `None` and zero are false.
fn truthy(value: Option<f64>) -> Option<f64> {
    value.filter(|value| *value != 0.0)
}

/// The value of one range at `quality` (scmdb, `product_stats._lerp`).
fn lerp(range: &Modifier, quality: f64) -> f64 {
    let span = range.end_quality - range.start_quality;
    if span <= 0.0 {
        return range.at_start;
    }
    let share = ((quality - range.start_quality) / span).clamp(0.0, 1.0);
    range.at_start + share * (range.at_end - range.at_start)
}

/// The combined effect of all slots of the first tier of `recipe`;
/// `quality_of` gives the quality of a slot by its index and the slot, `None`
/// for the base value 500.
pub fn combined(recipe: &Recipe, mut quality_of: impl FnMut(usize, &Slot) -> Option<f64>) -> Factors {
    let mut factors = Factors::default();
    let Some(tier) = recipe.tiers.first() else { return factors };
    for (index, slot) in tier.slots.iter().enumerate() {
        let quality = quality_of(index, slot).unwrap_or(BASE_QUALITY);
        let mut groups: Vec<(&str, Vec<&Modifier>)> = Vec::new();
        for modifier in slot.all_modifiers() {
            match groups.iter_mut().find(|(key, _)| *key == modifier.property_key) {
                Some((_, ranges)) => ranges.push(modifier),
                None => groups.push((&modifier.property_key, vec![modifier])),
            }
        }
        for (key, ranges) in groups {
            let hit = ranges
                .iter()
                .find(|range| range.start_quality <= quality && quality <= range.end_quality)
                .or(ranges.last())
                .copied();
            let Some(hit) = hit else { continue };
            let value = lerp(hit, quality);
            if ranges[0].kind == ModifierKind::Additive {
                // Python's round() rounds half to even; pips are small.
                let pips = value.round_ties_even() as i64;
                *factors.pips.entry(key.to_string()).or_default() += pips;
            } else {
                *factors.mul.entry(key.to_string()).or_insert(1.0) += value - 1.0;
            }
        }
    }
    factors
}

/// The impact damage, else the detonation damage, and whether it is the
/// detonation (scmdb `kr`).
pub fn damage(ammo: Option<&Ammo>) -> (Option<Damage>, bool) {
    let Some(ammo) = ammo else { return (None, false) };
    match (ammo.damage, ammo.detonation) {
        (None, None) => (None, false),
        (Some(impact), Some(blast)) => {
            if impact.total() < 1.0 {
                (Some(blast), true)
            } else {
                (Some(impact), false)
            }
        }
        (Some(impact), None) => (Some(impact), false),
        (None, Some(blast)) => (Some(blast), true),
    }
}

/// The damage per second of a weapon, `None` when it has no fire mode or no
/// damage (scmdb `qo`).
///
/// ⚠ One bullet of a shot: a scattergun shoots several, see [`pellets`].
pub fn dps(item: &ItemValues) -> Option<f64> {
    let weapon = item.weapon.as_ref()?;
    let (damage, _) = damage(weapon.ammo.as_ref());
    let damage = damage?;
    let entry = dps_entry(weapon)?;
    let rate = truthy(entry.fire_rate).or(truthy(entry.delay))?;
    Some(rate / 60.0 * damage.total() * truthy(entry.damage_multiplier).unwrap_or(1.0))
}

/// The fire action the damage per second counts: the first action that fires, else the first entry of a sequence
/// that fires or has a delay.
fn dps_entry(weapon: &crate::values::Weapon) -> Option<&FireAction> {
    if weapon.fire_modes.is_empty() {
        return None;
    }
    let fires = |action: &FireAction| action.fire_rate.unwrap_or(0.0) > 0.0;
    weapon.fire_modes.iter().map(|mode| &mode.action).find(|action| fires(action)).or_else(|| {
        weapon
            .fire_modes
            .iter()
            .filter(|mode| mode.action.kind == "sequence")
            .find_map(|mode| mode.sequence.iter().find(|entry| fires(entry) || entry.delay.unwrap_or(0.0) > 0.0))
    })
}

/// The bullets of one shot of the weapon whose damage per second [`dps`] counts: a scattergun shoots several, each
/// with the damage of the ammunition (Salvation Distortion Scattergun: 8 × 60 = 480, as erkul.games counts). 1
/// where the weapon shoots one or nothing is known.
pub fn pellets(item: &ItemValues) -> f64 {
    item.weapon
        .as_ref()
        .and_then(dps_entry)
        .and_then(|entry| entry.pellet_count)
        .filter(|count| *count > 1)
        .map_or(1.0, |count| count as f64)
}

/// The item after the quality of its materials.
pub fn crafted(item: &ItemValues, factors: &Factors) -> ItemValues {
    if item.cg_item_type.as_deref() == Some("WeaponMining") {
        return apply_component(item, factors);
    }
    match item.item_type {
        ItemType::Weapon => apply_weapon(item, factors),
        ItemType::ShipComponent => apply_component(item, factors),
        ItemType::Armor | ItemType::Other => apply_armor(item, factors),
    }
}

/// The item `recipe` makes, before and after `quality_of` (see [`combined`]).
pub fn craft(item: &ItemValues, recipe: &Recipe, quality_of: impl FnMut(usize, &Slot) -> Option<f64>) -> Crafted {
    let factors = combined(recipe, quality_of);
    Crafted { base: item.clone(), made: crafted(item, &factors), factors }
}

/// Fire rate, damage and recoil smoothing (scmdb `Km`), and the integrity.
fn apply_weapon(item: &ItemValues, factors: &Factors) -> ItemValues {
    let mut out = item.clone();
    if let Some(health) = factors.factor("health_maxhealth")
        && let Some(value) = out.stats.get_mut(&Stat::Health)
    {
        *value *= health;
    }
    let Some(weapon) = out.weapon.as_mut() else { return out };
    if let Some(rate) = truthy(factors.factor("weapon_firerate"))
        && !weapon.fire_modes.is_empty()
    {
        for mode in &mut weapon.fire_modes {
            if mode.is_sequence() {
                for entry in &mut mode.sequence {
                    entry.fire_rate = truthy(entry.fire_rate).map(|value| value * rate).or(entry.fire_rate);
                    entry.delay = truthy(entry.delay).map(|value| value * rate).or(entry.delay);
                }
            } else {
                mode.action.fire_rate = Some(mode.action.fire_rate.unwrap_or(0.0) * rate);
            }
        }
    }
    if let Some(damage) = truthy(factors.factor("weapon_damage"))
        && let Some(ammo) = weapon.ammo.as_mut()
    {
        ammo.damage = ammo.damage.map(|table| table.scaled(damage));
        ammo.detonation = ammo.detonation.map(|table| table.scaled(damage));
    }
    let smooth = truthy(factors.factor("weapon_recoil_smoothness")).unwrap_or(1.0);
    if smooth != 1.0 {
        let scale = |action: &mut FireAction| {
            if let Some(recoil) = action.recoil.as_mut() {
                for value in [&mut recoil.yaw_max_deg, &mut recoil.pitch_max_deg, &mut recoil.smooth_time] {
                    *value = value.map(|value| value * smooth);
                }
            }
        };
        for mode in &mut weapon.fire_modes {
            if mode.is_sequence() {
                mode.sequence.iter_mut().for_each(scale);
            } else {
                scale(&mut mode.action);
            }
        }
    }
    out
}

/// The component stats, the power pips and the beams of a mining laser.
fn apply_component(item: &ItemValues, factors: &Factors) -> ItemValues {
    let mut out = item.clone();
    for (key, stat) in COMPONENT_KEYS {
        if let Some(factor) = factors.factor(key)
            && let Some(value) = out.stats.get_mut(&stat)
        {
            *value *= factor;
        }
    }
    if let Some(pips) = factors.pips.get("itemresource_powergeneration").filter(|pips| **pips != 0)
        && let Some(value) = out.stats.get_mut(&Stat::PowerOutput)
    {
        *value += *pips as f64;
    }
    if let Some(damage) = factors.factor("weapon_damage") {
        for beam in &mut out.beams {
            *beam *= damage;
        }
    }
    out
}

/// Damage mitigation, temperature and radiation dissipation (scmdb `Wm`).
fn apply_armor(item: &ItemValues, factors: &Factors) -> ItemValues {
    let mut out = item.clone();
    let Some(armor) = out.armor.as_mut() else { return out };
    if let Some(mitigation) = truthy(factors.factor("armor_damagemitigation")) {
        for cell in &mut armor.resistance {
            if let Some(multiplier) = truthy(*cell) {
                *cell = Some(1.0 - (1.0 - multiplier) * mitigation);
            }
        }
    }
    if let Some((min, max)) = armor.temperature {
        let low = truthy(factors.factor("armor_temperaturemin")).unwrap_or(1.0);
        let high = truthy(factors.factor("armor_temperaturemax")).unwrap_or(1.0);
        armor.temperature = Some((min * low, max * high));
    }
    // With a radiation block, a missing rate counts as 0 (Python `or 0`).
    if let Some(dissipation) = truthy(factors.factor("armor_radiationdissipation"))
        && (armor.radiation_capacity.is_some() || armor.radiation_dissipation.is_some())
    {
        armor.radiation_dissipation = Some(armor.radiation_dissipation.unwrap_or(0.0) * dissipation);
    }
    out
}

/// The range `quality` falls into, else the one with the nearest limit
/// (`crafting._range_for`).
fn range_for<'m>(ranges: &[&'m Modifier], quality: f64) -> Option<&'m Modifier> {
    if let Some(hit) = ranges.iter().find(|range| range.start_quality <= quality && quality <= range.end_quality) {
        return Some(hit);
    }
    let distance = |range: &&Modifier| (quality - range.start_quality).abs().min((quality - range.end_quality).abs());
    ranges.iter().copied().min_by(|a, b| distance(a).total_cmp(&distance(b)))
}

/// The factor of one property at `quality`, linear in the matching range
/// and not extrapolated beyond it (`crafting.factor`); `None` without ranges.
pub fn factor(ranges: &[&Modifier], quality: f64) -> Option<f64> {
    let range = range_for(ranges, quality)?;
    if range.end_quality == range.start_quality {
        return Some(range.at_end);
    }
    let share = ((quality - range.start_quality) / (range.end_quality - range.start_quality)).clamp(0.0, 1.0);
    Some(range.at_start + share * (range.at_end - range.at_start))
}

/// The first range by start quality and the last one by end quality.
fn ends<'m>(ranges: &[&'m Modifier]) -> Option<(&'m Modifier, &'m Modifier)> {
    let first = ranges.iter().copied().reduce(|a, b| if b.start_quality < a.start_quality { b } else { a })?;
    let last = ranges.iter().copied().reduce(|a, b| if b.end_quality > a.end_quality { b } else { a })?;
    Some((first, last))
}

/// Whether better quality raises the value (`crafting.higher_is_better`):
/// the end of the last range against the start of the first. Without
/// ranges, true.
pub fn higher_is_better(ranges: &[&Modifier]) -> bool {
    ends(ranges).is_none_or(|(first, last)| last.at_end >= first.at_start)
}

/// Whether the modifier is a whole number rather than a factor
/// (`crafting.is_absolute`): a factor is always above zero.
pub fn is_absolute(ranges: &[&Modifier]) -> bool {
    ranges.iter().any(|range| range.at_start <= 0.0 || range.at_end <= 0.0)
}

/// What a property can reach (`crafting.range_of`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reach {
    /// The lowest quality.
    pub quality_from: f64,
    /// The highest quality.
    pub quality_to: f64,
    /// The factor at the lowest quality.
    pub factor_from: f64,
    /// The factor at the highest quality.
    pub factor_to: f64,
    /// The quality at which the factor is exactly 1, `None` when no range
    /// holds it.
    pub base: Option<f64>,
}

/// What a property can reach over all its ranges; `None` without ranges.
pub fn range_of(ranges: &[&Modifier]) -> Option<Reach> {
    let (first, last) = ends(ranges)?;
    let mut sorted = ranges.to_vec();
    sorted.sort_by(|a, b| a.start_quality.total_cmp(&b.start_quality));
    let base = sorted.iter().find_map(|range| {
        let (a, b) = (range.at_start, range.at_end);
        (a != b && a.min(b) <= 1.0 && 1.0 <= a.max(b))
            .then(|| range.start_quality + (1.0 - a) / (b - a) * (range.end_quality - range.start_quality))
    });
    Some(Reach {
        quality_from: first.start_quality,
        quality_to: last.end_quality,
        factor_from: first.at_start,
        factor_to: last.at_end,
        base,
    })
}

/// The shown values of a weapon or shield, as the hangar cards show them.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Summary {
    /// The damage of one shot (alpha); impact, else detonation.
    pub alpha: Option<f64>,
    /// Shots per minute of the first fire mode.
    pub fire_rate: Option<f64>,
    /// The damage per second.
    pub dps: Option<f64>,
    /// The range: projectile speed × lifetime, in m.
    pub range: Option<f64>,
    /// The integrity in HP.
    pub health: Option<f64>,
    /// The shield strength in HP.
    pub shield_health: Option<f64>,
    /// The shield regeneration per second.
    pub shield_regen: Option<f64>,
}

/// The shown values of `item`.
pub fn summary(item: &ItemValues) -> Summary {
    let weapon = item.weapon.as_ref();
    let ammo = weapon.and_then(|weapon| weapon.ammo.as_ref());
    let shown = weapon.and_then(|weapon| weapon.fire_modes.first()).map(crate::values::FireMode::shown);
    Summary {
        alpha: damage(ammo).0.map(|damage| damage.total()),
        fire_rate: shown.and_then(|action| truthy(action.fire_rate).or(truthy(action.delay))),
        dps: dps(item),
        range: ammo.and_then(Ammo::range),
        health: item.stat(Stat::Health),
        shield_health: item.stat(Stat::ShieldHealth),
        shield_regen: item.stat(Stat::ShieldRegen),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{FireMode, RecipeTier, SlotOption, Weapon};

    fn range(key: &str, start: f64, end: f64, at_start: f64, at_end: f64) -> Modifier {
        Modifier {
            property_key: key.to_string(),
            start_quality: start,
            end_quality: end,
            at_start,
            at_end,
            ..Modifier::default()
        }
    }

    fn slot(material: &str, modifiers: Vec<Modifier>) -> Slot {
        Slot {
            name: material.to_string(),
            options: vec![SlotOption { material: Some(material.to_string()), ..SlotOption::default() }],
            modifiers,
        }
    }

    /// The M5A Cannon S3 of build 4.10.2: Frame on integrity ±10 %, Emitter
    /// and Aperture Iris on impact force ±5 % each (checked against Erkul).
    fn m5a() -> (ItemValues, Recipe) {
        let recipe = Recipe {
            tag: "BP_CRAFT_BEHR_LaserCannon_S3".into(),
            tiers: vec![RecipeTier {
                craft_time_seconds: Some(1680),
                slots: vec![
                    slot("Agricium", vec![range("health_maxhealth", 0.0, 1000.0, 0.9, 1.1)]),
                    slot("Hadanite", vec![range("weapon_damage", 0.0, 1000.0, 0.95, 1.05)]),
                    slot("Dolivine", vec![range("weapon_damage", 0.0, 1000.0, 0.95, 1.05)]),
                ],
            }],
            ..Recipe::default()
        };
        let action = FireAction {
            name: Some("Single".into()),
            kind: "single".into(),
            fire_rate: Some(100.0),
            delay: Some(100.0),
            damage_multiplier: Some(1.0),
            ..FireAction::default()
        };
        let item = ItemValues {
            item_type: ItemType::Weapon,
            cg_item_type: Some("WeaponGun".into()),
            weapon: Some(Weapon {
                fire_modes: vec![FireMode {
                    action: FireAction {
                        name: Some("Single".into()),
                        kind: "sequence".into(),
                        ..FireAction::default()
                    },
                    sequence: vec![action],
                }],
                ammo: Some(Ammo {
                    speed: Some(1184.0),
                    lifetime: Some(2.03),
                    damage: Some(Damage([0.0, 410.184, 0.0, 0.0, 0.0, 0.0])),
                    detonation: None,
                }),
                magazine: Some(0),
            }),
            stats: BTreeMap::from([(Stat::Health, 1024.0)]),
            ..ItemValues::default()
        };
        (item, recipe)
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn base_quality_changes_nothing() {
        let (item, recipe) = m5a();
        let crafted = craft(&item, &recipe, |_, _| None);
        assert_eq!(crafted.made, item);
        assert!(close(crafted.factors.factor("weapon_damage"), 1.0));
    }

    #[test]
    fn slots_on_the_same_property_are_added() {
        let (item, recipe) = m5a();
        // Both damage slots at 1000: 1 + 0.05 + 0.05 = 1.10, not 1.05².
        let crafted = craft(&item, &recipe, |_, _| Some(1000.0));
        assert!(close(crafted.factors.factor("weapon_damage"), 1.1));
        assert!(close(crafted.factors.factor("health_maxhealth"), 1.1));
        let made = summary(&crafted.made);
        assert!(close(made.alpha, 410.184 * 1.1));
        assert!(close(made.dps, 100.0 / 60.0 * 410.184 * 1.1));
        assert!(close(made.health, 1024.0 * 1.1));
        // Quality changes neither fire rate nor range.
        assert!(close(made.fire_rate, 100.0));
        assert!(close(made.range, 1184.0 * 2.03));
    }

    #[test]
    fn the_quality_is_per_slot() {
        let (item, recipe) = m5a();
        let crafted = craft(&item, &recipe, |index, _| Some(if index == 1 { 0.0 } else { 500.0 }));
        assert!(close(crafted.factors.factor("weapon_damage"), 0.95));
        assert!(close(crafted.factors.factor("health_maxhealth"), 1.0));
    }

    #[test]
    fn outside_every_range_the_last_one_counts() {
        let ranges = [range("x", 0.0, 500.0, 0.8, 1.0), range("x", 501.0, 900.0, 1.0, 1.2)];
        let recipe = Recipe {
            tiers: vec![RecipeTier { slots: vec![slot("Iron", ranges.to_vec())], ..RecipeTier::default() }],
            ..Recipe::default()
        };
        // 950 lies in no range: the last one, clamped at its end.
        assert!(close(combined(&recipe, |_, _| Some(950.0)).factor("x"), 1.2));
        // 500.5 lies in no range either: the last one, clamped at its start.
        assert!(close(combined(&recipe, |_, _| Some(500.5)).factor("x"), 1.0));
        assert!(close(combined(&recipe, |_, _| Some(250.0)).factor("x"), 0.9));
    }

    #[test]
    fn pips_are_rounded_half_to_even_and_summed() {
        let mut pips = range("itemresource_powergeneration", 0.0, 1000.0, -3.0, 3.0);
        pips.kind = ModifierKind::Additive;
        let recipe = Recipe {
            tiers: vec![RecipeTier {
                slots: vec![slot("A", vec![pips.clone()]), slot("B", vec![pips])],
                ..RecipeTier::default()
            }],
            ..Recipe::default()
        };
        // 750 gives 1.5 per slot: Python's round() makes it 2, twice.
        assert_eq!(combined(&recipe, |_, _| Some(750.0)).pips["itemresource_powergeneration"], 4);
        // 250 gives -1.5 per slot: -2, twice.
        assert_eq!(combined(&recipe, |_, _| Some(250.0)).pips["itemresource_powergeneration"], -4);
        // 0.5 rounds to 0, not 1.
        let mut half = range("itemresource_powergeneration", 0.0, 1000.0, 0.0, 2.0);
        half.kind = ModifierKind::Additive;
        let recipe = Recipe {
            tiers: vec![RecipeTier { slots: vec![slot("A", vec![half])], ..RecipeTier::default() }],
            ..Recipe::default()
        };
        assert_eq!(combined(&recipe, |_, _| Some(250.0)).pips["itemresource_powergeneration"], 0);
    }

    #[test]
    fn factor_takes_the_nearest_range_outside() {
        let low = range("x", 0.0, 500.0, 0.8, 0.95);
        let high = range("x", 600.0, 1000.0, 1.05, 1.2);
        // 540 lies nearer the end of the lower range, 590 the start of the upper.
        assert!(close(factor(&[&low, &high], 540.0), 0.95));
        assert!(close(factor(&[&low, &high], 590.0), 1.05));
        assert!(close(factor(&[&low, &high], 800.0), 1.125));
        let flat = range("x", 300.0, 300.0, 0.5, 0.7);
        assert!(close(factor(&[&flat], 300.0), 0.7));
        assert_eq!(factor(&[], 300.0), None);
    }

    #[test]
    fn direction_absolute_and_reach() {
        let recoil = range("weapon_recoil_kick", 0.0, 1000.0, 1.2, 0.8);
        assert!(!higher_is_better(&[&recoil]));
        assert!(!is_absolute(&[&recoil]));
        let reach = range_of(&[&recoil]).unwrap();
        assert!(close(reach.base, 500.0));
        let pips = range("itemresource_powergeneration", 0.0, 1000.0, -3.0, 3.0);
        assert!(is_absolute(&[&pips]));
    }

    #[test]
    fn detonation_counts_when_impact_is_below_one() {
        let ammo = Ammo {
            damage: Some(Damage([0.5, 0.0, 0.0, 0.0, 0.0, 0.0])),
            detonation: Some(Damage([0.0, 0.0, 0.0, 300.0, 0.0, 0.0])),
            ..Ammo::default()
        };
        let (damage, detonation) = damage(Some(&ammo));
        assert!(detonation);
        assert_eq!(damage.unwrap().single_kind(), Some("thermal"));
    }
}
