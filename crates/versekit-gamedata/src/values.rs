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

//! The game values of crafted items and their recipes: weapons, shields and
//! components with their base values, and each recipe slot with the
//! modifiers its quality applies.
//!
//! The same model is filled from two sources: the `Data.p4k` of the user
//! ([`GameData::values`](crate::GameData::values)) and, as the fallback and
//! for the standing cross-check, the scmdb files of the same build
//! ([`crate::scmdb`]); [`crate::value_source`] chooses. What quality makes of a value is computed in
//! [`crate::quality`]; known errors in CIG's data are corrected with
//! [`crate::corrections`].

use std::collections::{BTreeMap, HashMap};

/// The six kinds of damage, in the order of every [`Damage`] table.
pub const DAMAGE_KINDS: [&str; 6] = ["physical", "energy", "distortion", "thermal", "biochemical", "stun"];

/// Damage by kind, in the order of [`DAMAGE_KINDS`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Copy, Default, PartialEq)]
pub struct Damage(pub [f64; 6]);

impl Damage {
    /// The damage of all kinds together.
    pub fn total(&self) -> f64 {
        // Python's `sum` over floats is compensated since 3.12: the last bit of an alpha must be the same.
        versekit_core::text::py_sum(self.0)
    }

    /// Each kind multiplied by `factor`.
    #[must_use]
    pub fn scaled(&self, factor: f64) -> Self {
        Self(self.0.map(|value| value * factor))
    }

    /// The one kind that does damage, `None` for none or several.
    pub fn single_kind(&self) -> Option<&'static str> {
        let mut hits = DAMAGE_KINDS.iter().zip(self.0).filter(|(_, value)| *value > 0.0);
        let first = hits.next()?;
        hits.next().is_none().then_some(*first.0)
    }
}

/// The recoil of a fire mode that quality can smooth.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Copy, Default, PartialEq)]
pub struct Recoil {
    /// The largest yaw kick in degrees.
    pub yaw_max_deg: Option<f64>,
    /// The largest pitch kick in degrees.
    pub pitch_max_deg: Option<f64>,
    /// The time the kick takes to smooth out, in seconds.
    pub smooth_time: Option<f64>,
}

/// One way a weapon fires: a single, rapid, burst, charged or beam action,
/// or one entry of a sequence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct FireAction {
    /// The name, e.g. `Single` or `Rapid`.
    pub name: Option<String>,
    /// The kind, e.g. `single`, `rapid`, `burst`, `charged`, `beam`; a
    /// sequence has `sequence`.
    pub kind: String,
    /// Shots per minute.
    pub fire_rate: Option<f64>,
    /// For a sequence entry: the delay, in the unit the data names (mostly
    /// shots per minute).
    pub delay: Option<f64>,
    /// The factor on the damage of each shot.
    pub damage_multiplier: Option<f64>,
    /// Projectiles per shot.
    pub pellet_count: Option<i64>,
    /// Heat per shot.
    pub heat_per_shot: Option<f64>,
    /// The spread in degrees: smallest and largest.
    pub spread: Option<(f64, f64)>,
    /// The recoil.
    pub recoil: Option<Recoil>,
}

/// A fire mode: one action, or a sequence of actions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct FireMode {
    /// The action itself; for a sequence its name and the kind `sequence`.
    pub action: FireAction,
    /// The entries of a sequence, in stored order; empty for other modes.
    pub sequence: Vec<FireAction>,
}

impl FireMode {
    /// Whether the mode is a sequence with entries.
    pub fn is_sequence(&self) -> bool {
        self.action.kind == "sequence" && !self.sequence.is_empty()
    }

    /// The action that counts for the shown values: the first entry of a
    /// sequence, else the action itself (as scmdb shows it).
    pub fn shown(&self) -> &FireAction {
        if self.is_sequence() { &self.sequence[0] } else { &self.action }
    }
}

/// The ammunition of a weapon.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct Ammo {
    /// The speed of a projectile in m/s.
    pub speed: Option<f64>,
    /// How long a projectile flies, in seconds.
    pub lifetime: Option<f64>,
    /// The damage on impact.
    pub damage: Option<Damage>,
    /// The damage of the detonation.
    pub detonation: Option<Damage>,
}

impl Ammo {
    /// The range: speed × lifetime, `None` when either is missing or zero.
    pub fn range(&self) -> Option<f64> {
        match (self.speed, self.lifetime) {
            (Some(speed), Some(lifetime)) if speed > 0.0 && lifetime > 0.0 => Some(speed * lifetime),
            _ => None,
        }
    }
}

/// The values of a weapon.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct Weapon {
    /// The fire modes in stored order.
    pub fire_modes: Vec<FireMode>,
    /// The ammunition.
    pub ammo: Option<Ammo>,
    /// The rounds of the magazine.
    pub magazine: Option<i64>,
}

/// A value of a component that quality can scale, with the scmdb field name
/// it stands for.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stat {
    /// Integrity in HP (`health`).
    Health,
    /// Shield strength in HP (`shieldHealth`).
    ShieldHealth,
    /// Shield regeneration per second (`shieldRegen`).
    ShieldRegen,
    /// The delay before a downed shield regenerates, in seconds.
    DownedRegenDelay,
    /// The delay before a damaged shield regenerates, in seconds.
    DamagedRegenDelay,
    /// Cooling rate (`coolingRate`).
    CoolingRate,
    /// Quantum fuel requirement.
    QuantumFuelRequirement,
    /// Quantum drive speed in m/s.
    DriveSpeed,
    /// Smallest aim assist range in m.
    MinAimAssistRange,
    /// Largest aim assist range in m.
    MaxAimAssistRange,
    /// Salvage speed multiplier.
    SalvageSpeedMultiplier,
    /// Salvage radius multiplier.
    RadiusMultiplier,
    /// Salvage extraction efficiency.
    ExtractionEfficiency,
    /// Tractor beam force in N.
    MaxForce,
    /// Tractor beam full strength distance in m.
    FullStrengthDistance,
    /// Tractor beam maximum distance in m.
    MaxDistance,
    /// Tractor beam maximum volume.
    MaxVolume,
    /// Power output in pips.
    PowerOutput,
}

impl Stat {
    /// Every stat, in order.
    pub const ALL: [Self; 18] = [
        Self::Health,
        Self::ShieldHealth,
        Self::ShieldRegen,
        Self::DownedRegenDelay,
        Self::DamagedRegenDelay,
        Self::CoolingRate,
        Self::QuantumFuelRequirement,
        Self::DriveSpeed,
        Self::MinAimAssistRange,
        Self::MaxAimAssistRange,
        Self::SalvageSpeedMultiplier,
        Self::RadiusMultiplier,
        Self::ExtractionEfficiency,
        Self::MaxForce,
        Self::FullStrengthDistance,
        Self::MaxDistance,
        Self::MaxVolume,
        Self::PowerOutput,
    ];

    /// The field name in the scmdb item files.
    pub const fn scmdb_field(self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::ShieldHealth => "shieldHealth",
            Self::ShieldRegen => "shieldRegen",
            Self::DownedRegenDelay => "downedRegenDelay",
            Self::DamagedRegenDelay => "damagedRegenDelay",
            Self::CoolingRate => "coolingRate",
            Self::QuantumFuelRequirement => "quantumFuelRequirement",
            Self::DriveSpeed => "driveSpeed",
            Self::MinAimAssistRange => "minAimAssistRange",
            Self::MaxAimAssistRange => "maxAimAssistRange",
            Self::SalvageSpeedMultiplier => "salvageSpeedMultiplier",
            Self::RadiusMultiplier => "radiusMultiplier",
            Self::ExtractionEfficiency => "extractionEfficiency",
            Self::MaxForce => "maxForce",
            Self::FullStrengthDistance => "fullStrengthDistance",
            Self::MaxDistance => "maxDistance",
            Self::MaxVolume => "maxVolume",
            Self::PowerOutput => "powerOutput",
        }
    }
}

/// How much of a kind of damage armour lets through, from 0 to 1.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct Armor {
    /// The share let through, by kind in the order of [`DAMAGE_KINDS`].
    pub resistance: [Option<f64>; 6],
    /// The lowest and highest temperature in °C.
    pub temperature: Option<(f64, f64)>,
    /// The radiation capacity in REM.
    pub radiation_capacity: Option<f64>,
    /// The radiation dissipation in REM/s.
    pub radiation_dissipation: Option<f64>,
}

/// The broad kind of an item, as the scmdb item files name it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Copy, Default, PartialEq, Eq, Hash)]
pub enum ItemType {
    /// A ship or personal weapon.
    Weapon,
    /// A ship component: shield, cooler, power plant and so on.
    ShipComponent,
    /// Armour and clothing.
    Armor,
    /// Anything else.
    #[default]
    Other,
}

/// The values of one item.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct ItemValues {
    /// The GUID of the entity record, lower case.
    pub guid: String,
    /// The record name without its type, e.g. `BEHR_LaserCannon_S3`.
    pub class_name: String,
    /// The English name, `None` without one.
    pub name: Option<String>,
    /// The broad kind.
    pub item_type: ItemType,
    /// The attach type, e.g. `WeaponGun` or `Shield`.
    pub attach_type: Option<String>,
    /// The item type of the crafting data (scmdb `cgItemType`), mostly the
    /// attach type.
    pub cg_item_type: Option<String>,
    /// The size.
    pub size: Option<i64>,
    /// The grade.
    pub grade: Option<i64>,
    /// The English manufacturer name.
    pub manufacturer: Option<String>,
    /// The GUID of the manufacturer record, lower case, where the own
    /// reading knows it (scmdb names only the manufacturer).
    pub manufacturer_record: Option<String>,
    /// The weapon values, for a weapon.
    pub weapon: Option<Weapon>,
    /// The component values quality can scale.
    pub stats: BTreeMap<Stat, f64>,
    /// The damage per second of each beam of a mining laser.
    pub beams: Vec<f64>,
    /// The armour values, for armour.
    pub armor: Option<Armor>,
    /// The corrections applied to this item.
    pub corrected: Vec<Corrected>,
    /// What the table of product values shows besides the rest; filled from
    /// scmdb where the own reading does not read it.
    pub extra: ItemExtra,
}

/// The values the table of product values shows and quality does not change.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct ItemExtra {
    /// Numbers by their scmdb field name: `mass`, `powerDraw`, `emSignature` …
    pub numbers: BTreeMap<String, f64>,
    /// Texts by their scmdb field name: `componentClass`, `attachSubType`,
    /// `combatRange`.
    pub texts: BTreeMap<String, String>,
    /// The values of a mining module by their name.
    pub modifiers: BTreeMap<String, f64>,
    /// The ranges of each beam of a mining laser, in the order of
    /// [`ItemValues::beams`].
    pub beams: Vec<BeamInfo>,
}

/// What a beam of a mining laser does besides its damage per second.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct BeamInfo {
    /// What the beam hits: `ElectricArc` for the laser power, `Extraction`.
    pub hit_type: String,
    /// The range up to which the damage is full, in m.
    pub full_range: Option<f64>,
    /// The range from which the damage is zero, in m.
    pub zero_range: Option<f64>,
}

impl ItemValues {
    /// The value of `stat`, `None` when the item has none.
    pub fn stat(&self, stat: Stat) -> Option<f64> {
        self.stats.get(&stat).copied()
    }

    /// The number the scmdb field `name` holds: a quality stat, else an
    /// extra number.
    pub fn number(&self, name: &str) -> Option<f64> {
        Stat::ALL
            .into_iter()
            .find(|stat| stat.scmdb_field() == name)
            .and_then(|stat| self.stat(stat))
            .or_else(|| self.extra.numbers.get(name).copied())
    }
}

/// How a modifier changes its property.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Copy, Default, PartialEq, Eq)]
pub enum ModifierKind {
    /// A factor on the property.
    #[default]
    Multiplier,
    /// A whole number added to the property (power pips).
    Additive,
}

/// One linear range of a modifier: the factor at the start and at the end of
/// a quality range.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct Modifier {
    /// The property key, e.g. `weapon_damage`.
    pub property_key: String,
    /// The English property name, e.g. `Impact Force`.
    pub property_name: Option<String>,
    /// Factor or summand.
    pub kind: ModifierKind,
    /// The quality the range starts at.
    pub start_quality: f64,
    /// The quality the range ends at.
    pub end_quality: f64,
    /// The modifier at the start quality.
    pub at_start: f64,
    /// The modifier at the end quality.
    pub at_end: f64,
    /// The corrections applied to this range; empty when it is as CIG
    /// shipped it.
    pub corrected: Vec<Corrected>,
}

/// What a slot or an option takes: a resource or an item.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Copy, Default, PartialEq, Eq)]
pub enum MaterialKind {
    /// A resource in SCU.
    #[default]
    Resource,
    /// A number of items, e.g. hand-mined gems.
    Item,
}

/// One option of a slot: a material and how much of it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct SlotOption {
    /// Resource or item.
    pub kind: MaterialKind,
    /// The English name of the material, e.g. `Agricium` or `Hadanite`.
    pub material: Option<String>,
    /// The GUID of the resource or entity record.
    pub material_guid: Option<String>,
    /// SCU for a resource, a count for an item.
    pub quantity: Option<f64>,
    /// The lowest quality the material may have.
    pub min_quality: Option<i64>,
    /// The modifiers of the option itself.
    pub modifiers: Vec<Modifier>,
}

/// One slot of a recipe: the materials it takes and what their quality
/// changes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct Slot {
    /// The English slot name, e.g. `Frame`.
    pub name: String,
    /// The options in stored order; the first one counts.
    pub options: Vec<SlotOption>,
    /// The modifiers of the slot, one entry per range.
    pub modifiers: Vec<Modifier>,
}

impl Slot {
    /// The material of the first option, else an empty name.
    pub fn material(&self) -> &str {
        self.options.first().and_then(|option| option.material.as_deref()).unwrap_or_default()
    }

    /// The modifiers of the first option followed by those of the slot.
    pub fn all_modifiers(&self) -> impl Iterator<Item = &Modifier> {
        self.options.first().into_iter().flat_map(|option| &option.modifiers).chain(&self.modifiers)
    }
}

/// One tier of a recipe.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct RecipeTier {
    /// The craft time in seconds.
    pub craft_time_seconds: Option<i64>,
    /// The slots in stored order.
    pub slots: Vec<Slot>,
}

/// A crafting recipe: the blueprint record with its tiers.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct Recipe {
    /// The GUID of the blueprint record, lower case.
    pub guid: String,
    /// The record name without its type, e.g. `BP_CRAFT_BEHR_LaserCannon_S3`.
    pub tag: String,
    /// The GUID of the entity the recipe makes, lower case; `None` when the
    /// record holds none.
    pub product: Option<String>,
    /// Whether the product reference leads to no record (a reference CIG left
    /// without target).
    pub product_missing: bool,
    /// The kind of the product as scmdb names it (`weapons`, `armour`,
    /// `ammo`, `cooler` …), from the folder of the blueprint record; empty
    /// where the folder does not say.
    pub kind: String,
    /// The subkind within the kind (`pistol`, `laser`, the role of armour …);
    /// empty where there is none.
    pub subtype: String,
    /// The tiers in stored order.
    pub tiers: Vec<RecipeTier>,
    /// The corrections applied to the recipe itself (its product).
    pub corrected: Vec<Corrected>,
}

/// A value that was corrected: where, what CIG shipped, what is used now
/// and why.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct Corrected {
    /// The field that was corrected, e.g. `modifierAtStart` or `product`.
    pub field: String,
    /// The value in the game data, as text.
    pub raw: String,
    /// The value used instead, as text; empty when it is only marked.
    pub value: String,
    /// Why.
    pub reason: String,
    /// Where the correction comes from, e.g. `scmdb` or `versekit`.
    pub source: String,
}

/// What taking an item apart gives back.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Dismantle {
    /// The share of the material that comes back, from 0 to 1.
    pub efficiency: f64,
    /// The seconds taking apart takes.
    pub seconds: i64,
    /// The English names of the resources that never come back.
    pub resources: Vec<String>,
    /// The English names of the items that never come back.
    pub entities: Vec<String>,
}

impl Default for Dismantle {
    /// Half of the material, fifteen seconds, nothing that stays lost: what
    /// the game data says today, for a source that says nothing.
    fn default() -> Self {
        Self { efficiency: 0.5, seconds: 15, resources: Vec::new(), entities: Vec::new() }
    }
}

/// The limits of the job queues of a crafting station (the fabricator), as the game data state them.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CrafterLimits {
    /// How many jobs the station crafts at once.
    pub in_progress: i64,
    /// How many further jobs it holds.
    pub waiting: i64,
    /// How many finished jobs it lists.
    pub history: i64,
    /// The jobs the station takes apart at once and holds waiting; `None` where it has no such queue.
    pub dismantle: Option<(i64, i64)>,
}

/// The game values of one build.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
pub struct GameValues {
    /// The limits of the crafting station; `None` where the source does not state them.
    #[serde(default)]
    pub crafter_limits: Option<CrafterLimits>,
    /// What taking an item apart gives back.
    pub dismantle: Dismantle,
    /// The build in the form scmdb names it, e.g. `4.10.2-live.12913470`;
    /// `None` when it is unknown.
    pub build: Option<String>,
    /// The items, by entity GUID in lower case.
    pub items: HashMap<String, ItemValues>,
    /// The recipes, ordered by tag ignoring ASCII case.
    pub recipes: Vec<Recipe>,
    /// The records that could not be read: record path, GUID and reason.
    pub failed: Vec<String>,
}

impl GameValues {
    /// The item `recipe` makes, `None` when it is unknown.
    pub fn product(&self, recipe: &Recipe) -> Option<&ItemValues> {
        self.items.get(recipe.product.as_deref()?)
    }

    /// The recipe with the blueprint GUID or tag `wanted`, ignoring ASCII case.
    pub fn recipe(&self, wanted: &str) -> Option<&Recipe> {
        self.recipes
            .iter()
            .find(|recipe| recipe.guid.eq_ignore_ascii_case(wanted) || recipe.tag.eq_ignore_ascii_case(wanted))
    }
}
