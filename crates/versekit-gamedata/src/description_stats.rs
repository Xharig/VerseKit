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

//! The stock values the game text shows under the description of a weapon or
//! component (`versekit_core::injection::item_stats`), read from the
//! DataForge for every item of the kinds that block knows, crafted or not.
//!
//! Source paths, each checked on a record of LIVE 4.10.2:
//!
//! * weapons: the base values of [`item_values`] (fire modes, ammunition,
//!   magazine) and the `projectileParams.penetrationParams.basePenetrationDistance`
//!   of the ammunition;
//! * shields: `SCItemShieldGeneratorParams` (`MaxShieldHealth`,
//!   `MaxShieldRegen`, both delays, `ShieldResistance[]` and
//!   `ShieldAbsorption[]` with `Max`, in the order physical, energy,
//!   distortion);
//! * power, coolant and signatures: the state `Online` of
//!   `ItemResourceComponentParams` (`deltas[].consumption` and `.generation`
//!   with `resource` and `resourceAmountPerSecond.units` or
//!   `.standardResourceUnits`; `signatureParams.EMSignature.nominalSignature`
//!   and `IRSignature`);
//! * integrity and resistances of a component: `SHealthComponentParams`
//!   (`Health`, `DamageResistances.<Kind>Resistance.Multiplier`, the share of
//!   damage that comes through); its distortion pool:
//!   `SDistortionParams.Maximum`;
//! * quantum drives: `SCItemQuantumDriveParams` (`quantumFuelRequirement`,
//!   `params.driveSpeed`, `params.spoolUpTime`, `params.cooldownTime`);
//! * missiles: `SCItemMissileParams` (`explosionParams.damage`,
//!   `explosionParams.maxRadius`, `targetingParams.lockTime`,
//!   `targetingParams.lockRangeMax`, `GCSParams.linearSpeed`);
//! * armour: the `DamageResistanceMacro` `SCItemSuitArmorParams.damageResistance`
//!   links to, and `SCItemClothingParams` (`TemperatureResistance`,
//!   `RadiationResistance.MaximumRadiationCapacity`).

use p4kx_dataforge::{DataForge, RecordTree, StructValue};
use std::collections::HashSet;

use versekit_core::injection::item_stats::{self, kind, value};
use versekit_core::jsonio::{Map, Value};

use crate::Localization;
use crate::catalog::text_key;
use crate::fields::{Trees, component, locale_key, number, structs, structure, text};
use crate::{item_values, quality};

/// The record path prefix of the items.
const ITEM_PREFIX: &str = "libs/foundry/records/entities/scitem/";

/// The kinds of damage a resistance is shown for, with the fields that hold them.
const RESISTANCES: [(&str, &str); 3] = [
    (value::RESIST_PHYSICAL, "PhysicalResistance"),
    (value::RESIST_ENERGY, "EnergyResistance"),
    (value::RESIST_DISTORTION, "DistortionResistance"),
];

/// The kind of item the block knows for an attach type, `None` for any other.
pub fn kind_of(attach_type: &str) -> Option<&'static str> {
    match attach_type {
        "WeaponGun" => Some(kind::SHIP_WEAPON),
        "WeaponPersonal" => Some(kind::PERSONAL_WEAPON),
        "Shield" => Some(kind::SHIELD),
        "Cooler" => Some(kind::COOLER),
        "PowerPlant" => Some(kind::POWER_PLANT),
        "QuantumDrive" => Some(kind::QUANTUM_DRIVE),
        "Missile" => Some(kind::MISSILE),
        other if other.starts_with("Char_Armor_") => Some(kind::ARMOR),
        _ => None,
    }
}

/// The values of one item by their name ([`value`]).
type Values = Vec<(&'static str, f64)>;

fn put(values: &mut Values, name: &'static str, found: Option<f64>) {
    if let Some(found) = found.filter(|v| v.is_finite()) {
        values.push((name, found));
    }
}

/// The state the item is in while it runs: `Online`, else the first one.
fn running_state<'t, 'f>(tree: &'t RecordTree<'f>) -> Option<&'t StructValue<'f>> {
    let resources = component(tree, "ItemResourceComponentParams");
    structs(resources, "states")
        .find(|state| text(Some(state), "name").is_some_and(|name| name.eq_ignore_ascii_case("Online")))
        .or_else(|| structs(resources, "states").next())
}

/// The amount per second of `resource` the deltas of `state` take (`consumption`) or give (`generation`).
fn flow(state: Option<&StructValue<'_>>, side: &str, resource: &str) -> Option<f64> {
    let mut total = None;
    for delta in structs(state, "deltas") {
        let amount = structure(Some(delta), side);
        if !text(amount, "resource").is_some_and(|r| r.eq_ignore_ascii_case(resource)) {
            continue;
        }
        let unit = structure(amount, "resourceAmountPerSecond");
        if let Some(found) = number(unit, "units").or_else(|| number(unit, "standardResourceUnits")) {
            *total.get_or_insert(0.0) += found;
        }
    }
    total
}

/// Power drawn, EM and IR of the running state.
fn signature(tree: &RecordTree<'_>, values: &mut Values) {
    let state = running_state(tree);
    put(values, value::POWER, flow(state, "consumption", "Power"));
    let signatures = structure(state, "signatureParams");
    put(values, value::EM, number(structure(signatures, "EMSignature"), "nominalSignature"));
    put(values, value::IR, number(structure(signatures, "IRSignature"), "nominalSignature"));
}

/// The resistances of a table of `DamageResistanceEntry`s: one minus the share that comes through.
fn resistances(table: Option<&StructValue<'_>>, values: &mut Values) {
    for (name, field) in RESISTANCES {
        put(values, name, number(structure(table, field), "Multiplier").map(|share| 1.0 - share));
    }
}

/// Integrity, resistances and distortion pool of a component.
fn integrity(tree: &RecordTree<'_>, values: &mut Values) {
    let health = component(tree, "SHealthComponentParams");
    put(values, value::HEALTH, number(health, "Health"));
    resistances(structure(health, "DamageResistances"), values);
    put(values, value::DISTORTION_HP, number(component(tree, "SDistortionParams"), "Maximum"));
}

fn weapon(tree: &RecordTree<'_>, trees: &mut Trees<'_, '_>, texts: &Localization, values: &mut Values) {
    let item = item_values::read(tree, trees, texts);
    put(values, value::HEALTH, item.stats.get(&crate::values::Stat::Health).copied());
    let Some(gun) = item.weapon.as_ref() else { return };
    let shown = gun.fire_modes.iter().map(crate::values::FireMode::shown).find(|a| a.fire_rate.unwrap_or(0.0) > 0.0);
    // A shot of a scattergun is several pellets, each with the damage of the ammunition: alpha and damage per second
    // count all of them, as erkul.games shows them (Salvation Distortion Scattergun: 8 × 60 = 480), and so does the
    // table of the crafted item. `quality::dps` counts one pellet.
    let pellets = quality::pellets(&item);
    put(values, value::DPS, quality::dps(&item).map(|dps| dps * pellets));
    let (damage, _) = quality::damage(gun.ammo.as_ref());
    if let (Some(damage), Some(action)) = (damage, shown) {
        let multiplier = action.damage_multiplier.filter(|m| *m != 0.0).unwrap_or(1.0);
        put(values, value::ALPHA, Some(damage.total() * multiplier * pellets));
    }
    put(values, value::FIRE_RATE, shown.and_then(|a| a.fire_rate));
    put(values, value::SPREAD, shown.and_then(|a| a.spread).map(|(_, max)| max));
    if let Some(ammo) = gun.ammo.as_ref() {
        put(values, value::RANGE, ammo.range());
        put(values, value::SPEED, ammo.speed);
    }
    put(values, value::AMMO, gun.magazine.map(|m| m as f64));
    if let Some(ammo) = item_values::ammo_record(tree, trees) {
        let projectile = structure(Some(ammo.root()), "projectileParams");
        put(values, value::PENETRATION, number(structure(projectile, "penetrationParams"), "basePenetrationDistance"));
    }
    signature(tree, values);
}

fn shield(tree: &RecordTree<'_>, values: &mut Values) {
    let params = component(tree, "SCItemShieldGeneratorParams");
    put(values, value::SHIELD_HP, number(params, "MaxShieldHealth"));
    put(values, value::REGEN, number(params, "MaxShieldRegen"));
    put(values, value::DAMAGED_DELAY, number(params, "DamagedRegenDelay"));
    put(values, value::DOWNED_DELAY, number(params, "DownedRegenDelay"));
    // The entries name no kind of damage (only `Max` and `Min`): their place counts, in the order of the damage
    // table (physical, energy, distortion, …), as on a LIVE 4.10.2 shield.
    let names = [
        (value::RESIST_PHYSICAL, value::ABSORB_PHYSICAL),
        (value::RESIST_ENERGY, value::ABSORB_ENERGY),
        (value::RESIST_DISTORTION, value::ABSORB_DISTORTION),
    ];
    for ((resist, absorb), (resistance, absorption)) in
        names.into_iter().zip(structs(params, "ShieldResistance").zip(structs(params, "ShieldAbsorption")))
    {
        put(values, resist, number(Some(resistance), "Max"));
        put(values, absorb, number(Some(absorption), "Max"));
    }
    put(values, value::HEALTH, number(component(tree, "SHealthComponentParams"), "Health"));
    signature(tree, values);
}

fn cooler(tree: &RecordTree<'_>, values: &mut Values) {
    put(values, value::COOLING, flow(running_state(tree), "generation", "Coolant"));
    signature(tree, values);
    integrity(tree, values);
}

fn power_plant(tree: &RecordTree<'_>, values: &mut Values) {
    put(values, value::OUTPUT, flow(running_state(tree), "generation", "Power"));
    signature(tree, values);
    integrity(tree, values);
}

fn quantum_drive(tree: &RecordTree<'_>, values: &mut Values) {
    let drive = component(tree, "SCItemQuantumDriveParams");
    let params = structure(drive, "params");
    put(values, value::DRIVE_SPEED, number(params, "driveSpeed"));
    put(values, value::FUEL, number(drive, "quantumFuelRequirement"));
    put(values, value::SPOOL, number(params, "spoolUpTime"));
    put(values, value::COOLDOWN, number(params, "cooldownTime"));
    signature(tree, values);
    integrity(tree, values);
}

fn missile(tree: &RecordTree<'_>, values: &mut Values) {
    let params = component(tree, "SCItemMissileParams");
    let explosion = structure(params, "explosionParams");
    let damage = structure(explosion, "damage");
    let total =
        ["DamagePhysical", "DamageEnergy", "DamageDistortion", "DamageThermal", "DamageBiochemical", "DamageStun"]
            .into_iter()
            .filter_map(|field| number(damage, field))
            .sum::<f64>();
    put(values, value::DAMAGE, damage.map(|_| total));
    put(values, value::RADIUS, number(explosion, "maxRadius"));
    let targeting = structure(params, "targetingParams");
    put(values, value::LOCK_TIME, number(targeting, "lockTime"));
    put(values, value::LOCK_RANGE, number(targeting, "lockRangeMax"));
    put(values, value::SPEED, number(structure(params, "GCSParams"), "linearSpeed"));
}

fn armor(tree: &RecordTree<'_>, trees: &mut Trees<'_, '_>, values: &mut Values) {
    let suit = component(tree, "SCItemSuitArmorParams");
    if let Some(macro_record) = trees.follow(suit, "damageResistance") {
        resistances(structure(Some(macro_record.root()), "damageResistance"), values);
    }
    let clothing = component(tree, "SCItemClothingParams");
    let temperature = structure(clothing, "TemperatureResistance");
    put(values, value::TEMP_MIN, number(temperature, "MinResistance"));
    put(values, value::TEMP_MAX, number(temperature, "MaxResistance"));
    put(values, value::RADIATION, number(structure(clothing, "RadiationResistance"), "MaximumRadiationCapacity"));
}

/// The values of the item `tree` describes when it is of a kind the block
/// knows: (kind, values).
pub(crate) fn values_of(
    tree: &RecordTree<'_>,
    trees: &mut Trees<'_, '_>,
    texts: &Localization,
) -> Option<(&'static str, Values)> {
    let definition = structure(component(tree, "SAttachableComponentParams"), "AttachDef");
    let found = kind_of(text(definition, "Type")?)?;
    let mut values = Values::new();
    match found {
        kind::SHIP_WEAPON | kind::PERSONAL_WEAPON => weapon(tree, trees, texts, &mut values),
        kind::SHIELD => shield(tree, &mut values),
        kind::COOLER => cooler(tree, &mut values),
        kind::POWER_PLANT => power_plant(tree, &mut values),
        kind::QUANTUM_DRIVE => quantum_drive(tree, &mut values),
        kind::MISSILE => missile(tree, &mut values),
        kind::ARMOR => armor(tree, trees, &mut values),
        _ => {}
    }
    Some((found, values))
}

/// One item with a description key: its keys, and its kind and values when it has any.
struct Found {
    guid: String,
    name: Option<String>,
    text: String,
    values: Option<(&'static str, Values)>,
}

impl Found {
    fn entry(self) -> (String, Value) {
        let mut entry = Map::new();
        if let Some(name) = &self.name {
            entry.push(("name".into(), Value::str(name)));
        }
        entry.push(("text".into(), Value::str(&self.text)));
        if let Some((found_kind, values)) = self.values {
            entry.push(("art".into(), Value::str(found_kind)));
            entry.push((
                "w".into(),
                Value::Object(values.into_iter().map(|(k, v)| (k.to_string(), Value::Float(v))).collect()),
            ));
        }
        (self.guid, Value::Object(entry))
    }
}

/// The stock values of every item of the DataForge of a kind the block knows,
/// in the form the catalog file keeps them (`item_stats::CATALOG_KEY`): record GUID in
/// lower case → `{"name": …, "text": …, "art": …, "w": {…}}`. An item without
/// a description key is left out. An item without values that shows the
/// description of an item with values is kept without `art` and `w`: the
/// description is then not the same for all its items and gets no block.
pub fn read(forge: &DataForge<'_>, texts: &Localization) -> Map {
    let mut trees = Trees::new(forge);
    let items: Vec<_> = forge
        .records()
        .filter(|record| {
            matches!(record.type_name(), Ok("EntityClassDefinition"))
                && record.record_path().is_ok_and(|path| crate::catalog::starts_with_ignore_case(path, ITEM_PREFIX))
        })
        .map(|record| record.guid())
        .collect();
    let mut all = Vec::new();
    for guid in items {
        let Some(tree) = trees.once(guid) else { continue };
        let localization =
            structure(structure(component(&tree, "SAttachableComponentParams"), "AttachDef"), "Localization");
        let Some(text) = text_key(locale_key(localization, "Description")) else { continue };
        let values = values_of(&tree, &mut trees, texts).filter(|(_, values)| !values.is_empty());
        let name = text_key(locale_key(localization, "Name"));
        all.push(Found { guid: guid.to_string().to_ascii_lowercase(), name, text, values });
    }
    let shown = |item: &Found| item_stats::shown_text_key(item.name.as_deref().unwrap_or(""), &item.text);
    let with_values: HashSet<String> = all.iter().filter(|item| item.values.is_some()).map(shown).collect();
    all.into_iter()
        .filter(|item| item.values.is_some() || with_values.contains(&shown(item)))
        .map(Found::entry)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kinds_follow_the_attach_type() {
        assert_eq!(kind_of("WeaponGun"), Some(kind::SHIP_WEAPON));
        assert_eq!(kind_of("WeaponPersonal"), Some(kind::PERSONAL_WEAPON));
        assert_eq!(kind_of("Shield"), Some(kind::SHIELD));
        assert_eq!(kind_of("Cooler"), Some(kind::COOLER));
        assert_eq!(kind_of("PowerPlant"), Some(kind::POWER_PLANT));
        assert_eq!(kind_of("QuantumDrive"), Some(kind::QUANTUM_DRIVE));
        assert_eq!(kind_of("Missile"), Some(kind::MISSILE));
        assert_eq!(kind_of("Char_Armor_Torso"), Some(kind::ARMOR));
        assert_eq!(kind_of("WeaponMining"), None, "mining lasers have values of their own");
        assert_eq!(kind_of("Radar"), None);
    }

    #[test]
    fn a_value_that_is_no_number_is_left_out() {
        let mut values = Values::new();
        put(&mut values, value::EM, Some(f64::NAN));
        put(&mut values, value::IR, None);
        put(&mut values, value::HEALTH, Some(230.0));
        assert_eq!(values, [(value::HEALTH, 230.0)]);
    }
}
