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

//! The base values of an item from its entity record: integrity, shield,
//! weapon fire modes and ammunition, read through the general DataForge
//! model.
//!
//! Replace with p4kx-export once krt-profit/p4k-extractor#317 lands (typed
//! stat blocks for weapons and shields); until then this module is the only
//! place that knows these source paths.
//!
//! Source paths (the same p4k-extractor#317 lists):
//!
//! * integrity: `SHealthComponentParams.Health`;
//! * shield: `SCItemShieldGeneratorParams` (`MaxShieldHealth`,
//!   `MaxShieldRegen`, `DownedRegenDelay`, `DamagedRegenDelay`);
//! * fire modes: `SCItemWeaponComponentParams.fireActions`, a sequence with
//!   its `sequenceEntries[].weaponAction`, a charged action with the one it
//!   fires; rate, heat and launcher (`pelletCount`, `damageMultiplier`,
//!   `spreadParams`); the recoil from the linked
//!   `WeaponProceduralRecoilConfigDef` (`curveAimRecoil`);
//! * ammunition: the `AmmoParams` record of the item's own
//!   `SAmmoContainerComponentParams` (ship weapons), else of its magazine
//!   (`ammoContainerRecord`, personal weapons): `speed`, `lifetime`,
//!   `projectileParams.damage` and the damage of its detonation.

use p4kx_dataforge::{RecordTree, StructValue};

use crate::Localization;
use crate::fields::{Trees, class_name, component, integer, number, structs, structure, text};
use crate::recipes::english;
use crate::values::{Ammo, DAMAGE_KINDS, Damage, FireAction, FireMode, ItemType, ItemValues, Recoil, Stat, Weapon};

/// The fields of a `DamageInfo`, in the order of [`DAMAGE_KINDS`].
const DAMAGE_FIELDS: [&str; 6] =
    ["DamagePhysical", "DamageEnergy", "DamageDistortion", "DamageThermal", "DamageBiochemical", "DamageStun"];

/// The attach types the scmdb files count as ship components.
const SHIP_COMPONENTS: [&str; 16] = [
    "PowerPlant",
    "Cooler",
    "Shield",
    "Radar",
    "QuantumDrive",
    "WeaponMining",
    "DockingCollar",
    "TractorBeam",
    "SalvageModifier",
    "Container",
    "MiningModifier",
    "SalvageHead",
    "FuelNozzle",
    "FuelIntake",
    "LifeSupportGenerator",
    "JumpDrive",
];

fn item_type(attach_type: Option<&str>) -> ItemType {
    match attach_type {
        Some("WeaponGun" | "WeaponPersonal") => ItemType::Weapon,
        Some(kind) if SHIP_COMPONENTS.contains(&kind) => ItemType::ShipComponent,
        Some(kind) if kind.starts_with("Char_Armor_") => ItemType::Armor,
        _ => ItemType::Other,
    }
}

fn damage(info: Option<&StructValue<'_>>) -> Option<Damage> {
    info?;
    let damage = Damage(DAMAGE_FIELDS.map(|name| number(info, name).unwrap_or(0.0)));
    debug_assert_eq!(DAMAGE_KINDS.len(), DAMAGE_FIELDS.len());
    Some(damage)
}

/// The damage of the detonation of a projectile, where it has one.
fn detonation(projectile: Option<&StructValue<'_>>) -> Option<Damage> {
    let params = structure(projectile, "detonationParams");
    let explosion = structure(params, "explosionParams");
    damage(structure(explosion, "damage")).filter(|damage| damage.total() > 0.0)
}

fn ammo(tree: &RecordTree<'_>) -> Ammo {
    let root = Some(tree.root());
    let projectile = structure(root, "projectileParams");
    Ammo {
        speed: number(root, "speed"),
        lifetime: number(root, "lifetime"),
        damage: damage(structure(projectile, "damage")),
        detonation: detonation(projectile),
    }
}

fn kind_of(type_name: &str) -> String {
    match type_name {
        "SWeaponActionFireSingleParams" => "single".into(),
        "SWeaponActionFireRapidParams" => "rapid".into(),
        "SWeaponActionFireBurstParams" => "burst".into(),
        "SWeaponActionFireChargedParams" => "charged".into(),
        "SWeaponActionFireBeamParams" => "beam".into(),
        "SWeaponActionSequenceParams" => "sequence".into(),
        other => other.trim_start_matches("SWeaponAction").trim_end_matches("Params").to_ascii_lowercase(),
    }
}

fn recoil(trees: &mut Trees<'_, '_>, action: Option<&StructValue<'_>>) -> Option<Recoil> {
    let record = trees.follow(action, "recoil")?;
    let aim = structure(structure(Some(record.root()), "weaponProceduralAimRecoil"), "curveAimRecoil");
    aim?;
    Some(Recoil {
        yaw_max_deg: number(aim, "yawMaxDegrees"),
        pitch_max_deg: number(aim, "pitchMaxDegrees"),
        smooth_time: number(aim, "recoilSmoothTime"),
    })
}

fn action(trees: &mut Trees<'_, '_>, value: &StructValue<'_>) -> FireAction {
    let kind = kind_of(value.type_name());
    // A charged action fires the action it holds; its launcher counts.
    let fired = if kind == "charged" { structure(Some(value), "weaponAction").unwrap_or(value) } else { value };
    let launch = structure(Some(fired), "launchParams");
    let spread = structure(launch, "spreadParams");
    FireAction {
        name: text(Some(value), "name").map(str::to_string),
        fire_rate: if kind == "charged" { None } else { number(Some(value), "fireRate") },
        delay: None,
        damage_multiplier: number(launch, "damageMultiplier"),
        pellet_count: integer(launch, "pelletCount"),
        heat_per_shot: if kind == "charged" { None } else { number(Some(value), "heatPerShot") },
        spread: number(spread, "min").zip(number(spread, "max")),
        recoil: if kind == "charged" { None } else { recoil(trees, Some(value)) },
        kind,
    }
}

/// The fire mode of one entry of `fireActions`; `None` for parallel
/// actions, which scmdb leaves out as well (several barrels at once).
fn fire_mode(trees: &mut Trees<'_, '_>, value: &StructValue<'_>) -> Option<FireMode> {
    // A dynamic condition fires its default action unless a condition holds.
    let value = if value.type_name() == "SWeaponActionDynamicConditionParams" {
        structure(Some(value), "defaultWeaponAction")?
    } else {
        value
    };
    if value.type_name() == "SWeaponActionParallelParams" {
        return None;
    }
    let own = action(trees, value);
    let entries: Vec<_> = structs(Some(value), "sequenceEntries").collect();
    let counts = delays_count(text(Some(value), "mode"), entries.len());
    let sequence = entries
        .into_iter()
        .filter_map(|entry| {
            let inner = structure(Some(entry), "weaponAction")?;
            let mut action = action(trees, inner);
            action.delay = number(Some(entry), "delay");
            action.fire_rate = limited_rate(action.fire_rate, action.delay, text(Some(entry), "unit"), counts);
            Some(action)
        })
        .collect();
    Some(FireMode { action: own, sequence })
}

/// Whether the delays between the entries of a sequence set its pace. A sequence that loops over one entry is
/// no sequence: the entry fires at its own rate and its delay is the pause before it fires again (the C-788:
/// rate 50, delay 40 per minute; erkul.games shows 50 shots a minute). Two entries or more, and
/// every other kind of sequence (`Individually`, `Automatically`), wait the delays out.
fn delays_count(mode: Option<&str>, entries: usize) -> bool {
    entries >= 2 || mode.is_some_and(|mode| mode != "Looping")
}

/// The rate an entry of a sequence fires at: its own, and where the delays count, a delay in shots per minute
/// limits it, the slower of the two counts. A delay in seconds does not.
fn limited_rate(fire_rate: Option<f64>, delay: Option<f64>, unit: Option<&str>, counts: bool) -> Option<f64> {
    match delay.filter(|delay| counts && unit == Some("RPM") && *delay > 0.0) {
        Some(delay) => Some(fire_rate.map_or(delay, |rate| rate.min(delay))),
        None => fire_rate,
    }
}

/// The ammunition container that holds the item's ammunition: its own, else
/// that of its magazine.
fn ammo_container<'t>(tree: &'t RecordTree<'_>, magazine: Option<&'t RecordTree<'_>>) -> Option<&'t StructValue<'t>> {
    let own = component(tree, "SAmmoContainerComponentParams");
    if own.is_some_and(|own| crate::fields::reference(Some(own), "ammoParamsRecord").is_some()) {
        return own;
    }
    magazine.and_then(|magazine| component(magazine, "SAmmoContainerComponentParams")).or(own)
}

/// The ammunition record of the weapon `tree`: the one its own container names, else the one of its magazine.
pub(crate) fn ammo_record<'f>(tree: &RecordTree<'_>, trees: &mut Trees<'f, '_>) -> Option<std::rc::Rc<RecordTree<'f>>> {
    let params = component(tree, "SCItemWeaponComponentParams")?;
    let magazine = trees.follow(Some(params), "ammoContainerRecord");
    let container = ammo_container(tree, magazine.as_deref());
    trees.follow(container, "ammoParamsRecord")
}

fn weapon(tree: &RecordTree<'_>, trees: &mut Trees<'_, '_>) -> Option<Weapon> {
    let params = component(tree, "SCItemWeaponComponentParams")?;
    let fire_modes = structs(Some(params), "fireActions").filter_map(|value| fire_mode(trees, value)).collect();
    let magazine = trees.follow(Some(params), "ammoContainerRecord");
    let container = ammo_container(tree, magazine.as_deref());
    let ammo = trees.follow(container, "ammoParamsRecord").map(|record| ammo(&record));
    Some(Weapon { fire_modes, ammo, magazine: integer(container, "maxAmmoCount") })
}

/// Whether the attach type of the item `tree` describes is one of `kinds`.
pub(crate) fn attach_type_is(tree: &RecordTree<'_>, kinds: &[&str]) -> bool {
    let definition = structure(component(tree, "SAttachableComponentParams"), "AttachDef");
    text(definition, "Type").is_some_and(|kind| kinds.contains(&kind))
}

/// The values of the item `tree` describes.
pub(crate) fn read(tree: &RecordTree<'_>, trees: &mut Trees<'_, '_>, texts: &Localization) -> ItemValues {
    let definition = structure(component(tree, "SAttachableComponentParams"), "AttachDef");
    let name = english(structure(definition, "Localization"), "Name", texts);
    let maker = trees.follow(definition, "Manufacturer");
    let manufacturer_record = maker.as_ref().map(|maker| maker.guid().to_string());
    let manufacturer = maker.and_then(|maker| {
        // Without a name text the short code, else the record name,
        // stands for the maker.
        english(structure(Some(maker.root()), "Localization"), "Name", texts)
            .or_else(|| text(Some(maker.root()), "Code").map(str::to_string))
            .or_else(|| Some(class_name(&maker).to_string()))
    });
    let attach_type = text(definition, "Type").map(str::to_string);
    let item_type = item_type(attach_type.as_deref());
    let mut stats = std::collections::BTreeMap::new();
    if let Some(health) = number(component(tree, "SHealthComponentParams"), "Health") {
        stats.insert(Stat::Health, health);
    }
    let shield = component(tree, "SCItemShieldGeneratorParams");
    for (stat, field) in [
        (Stat::ShieldHealth, "MaxShieldHealth"),
        (Stat::ShieldRegen, "MaxShieldRegen"),
        (Stat::DownedRegenDelay, "DownedRegenDelay"),
        (Stat::DamagedRegenDelay, "DamagedRegenDelay"),
    ] {
        if let Some(value) = number(shield, field) {
            stats.insert(stat, value);
        }
    }
    let weapon = if item_type == ItemType::Weapon { weapon(tree, trees) } else { None };
    ItemValues {
        guid: tree.guid().to_string(),
        class_name: class_name(tree).to_string(),
        name,
        item_type,
        cg_item_type: attach_type.clone(),
        attach_type,
        size: integer(definition, "Size"),
        grade: integer(definition, "Grade"),
        manufacturer,
        manufacturer_record,
        weapon,
        stats,
        beams: Vec::new(),
        armor: None,
        corrected: Vec::new(),
        extra: crate::values::ItemExtra::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sequences of the game data of 4.10.2 (read from a Data.p4k): mode, entries, rate and delay per entry.
    #[test]
    fn a_loop_over_one_entry_keeps_its_own_rate_and_a_real_sequence_waits_its_delays() {
        // C-788 Cannon: one entry, 50 shots a minute, delay 40 a minute: 50 (erkul.games shows 50).
        let one = delays_count(Some("Looping"), 1);
        assert!(!one);
        assert_eq!(limited_rate(Some(50.0), Some(40.0), Some("RPM"), one), Some(50.0));
        // 'WAR' Cannon, the other loop over one entry whose delay is below its rate.
        assert_eq!(limited_rate(Some(200.0), Some(50.0), Some("RPM"), one), Some(200.0));
        // Attrition-4 Repeater: two entries of 350 and 350: unchanged.
        let two = delays_count(Some("Looping"), 2);
        assert!(two);
        assert_eq!(limited_rate(Some(350.0), Some(350.0), Some("RPM"), two), Some(350.0));
        // DR Model-XJ1 Repeater: two entries, rate 400, delay 350: the slower one.
        assert_eq!(limited_rate(Some(400.0), Some(350.0), Some("RPM"), two), Some(350.0));
        // A delay above the rate never speeds an entry up, and a delay in seconds does not slow it.
        assert_eq!(limited_rate(Some(60.0), Some(100.0), Some("RPM"), two), Some(60.0));
        assert_eq!(limited_rate(Some(750.0), Some(1.0), Some("Seconds"), two), Some(750.0));
        // An entry without a rate takes the delay as its rate where the delays count.
        assert_eq!(limited_rate(None, Some(300.0), Some("RPM"), two), Some(300.0));
        assert_eq!(limited_rate(None, Some(300.0), Some("RPM"), one), None);
    }

    #[test]
    fn a_sequence_of_another_kind_than_looping_counts_its_delays_even_with_one_entry() {
        assert!(delays_count(Some("Automatically"), 1));
        assert!(delays_count(Some("Individually"), 2));
        assert!(!delays_count(None, 1));
        assert!(delays_count(None, 2));
    }
}
