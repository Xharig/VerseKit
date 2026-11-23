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

//! The names of the kinds of items the stock scan must know: the items of the blueprint catalog, the components of
//! ships, armour (with helmets, undersuits and backpacks) and the weapons of a person with their attachments and
//! magazines. The names come from the English `global.ini` of the game: an item has a name text (`item_Name…`) and,
//! under the same ending, a description (`item_Desc…`) whose line `Item Type:` (or `Type:`) says what it is.
//!
//! Left out on purpose: whatever the line does not name as one of these kinds (liveries, clothing, decoration,
//! coins, Vanduul pods, consumables). A card of such an item is not read to a name, the scan asks.

use std::collections::BTreeMap;

use super::GameTexts;
use crate::blueprint_scan::{Entry, Names};

/// The kind of an item the scan knows by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    /// An item of the blueprint catalog (also when it is a component, a weapon or armour).
    Blueprint,
    /// A component of a ship: coolers, shields, power plants, drives, radars, weapons, turrets, racks, …
    Component,
    /// Armour: helmets, arms, cores, legs, undersuits, backpacks.
    Armor,
    /// A weapon of a person, its attachments and its magazines.
    PersonalWeapon,
}

impl Group {
    /// A short name for the report of a measurement.
    pub fn label(self) -> &'static str {
        match self {
            Group::Blueprint => "blueprint",
            Group::Component => "component",
            Group::Armor => "armor",
            Group::PersonalWeapon => "personal weapon",
        }
    }
}

/// The `Item Type:` values of armour.
const ARMOR: &[&str] = &[
    "Light Armor",
    "Medium Armor",
    "Heavy Armor",
    "Super Heavy Armor",
    "Heavy Utility",
    "Light Helmet",
    "Flight Helmet",
    "Racing Helmet",
    "Undersuit",
    "Light Backpack",
    "Medium Backpack",
    "Heavy Backpack",
    "Ammo Carrier",
];

/// The `Item Type:` values of the weapons of a person, their attachments and magazines.
const PERSONAL_WEAPON: &[&str] = &[
    "Assault Rifle",
    "Rifle",
    "Pistol",
    "Frag Pistol",
    "SMG",
    "Shotgun",
    "LMG",
    "HMG",
    "Sniper Rifle",
    "Railgun",
    "Crossbow",
    "Launcher",
    "Grenade Launcher",
    "Grenade",
    "Knife",
    "Magazine",
    "Battery",
    "Telescopic",
    "Reflex",
    "Holographic",
    "Projection",
    "Suppressor",
    "Compensator",
    "Ballistic Compensator",
    "Energy Stabilizer",
    "Flash Hider",
    "Laser Pointer",
    "Flashlight",
];

/// The `Item Type:` values of the components of a ship that are not weapons.
const COMPONENT: &[&str] = &[
    "Cooler",
    "Shield Generator",
    "Power Plant",
    "Quantum Drive",
    "Quantum Dampener",
    "Quantum Enforcement Device",
    "Jump Module",
    "Radar",
    "Life Support",
    "Computer",
    "Missile Rack",
    "Bomb Rack",
    "Torpedo Rack",
    "Missile Launcher",
    "Torpedo Launcher",
    "Rocket Pod",
    "Weapon Mount",
    "Gimbal Mount",
    "Tractor Beam",
    "Towing Beam",
    "Decoy",
    "Noise",
    "Burst Generator",
    "EMP Generator",
    "Scraper Module",
    "Ammo",
    "PDC",
];

/// What ship weapons and turrets are called at the end of their `Item Type:` (`Laser Cannon`, `Ballistic Gatling`).
const WEAPON_ENDINGS: &[&str] =
    &["Cannon", "Canon", "Repeater", "Gatling", "Gatling Gun", "Gatling (x2)", "Scattergun", "Beam", "Turret"];

/// The group an `Item Type:` value belongs to; `None` where the scan does not need the item by name.
pub fn group_of_type(item_type: &str) -> Option<Group> {
    let item_type = item_type.trim();
    if ARMOR.contains(&item_type) {
        Some(Group::Armor)
    } else if PERSONAL_WEAPON.contains(&item_type) {
        Some(Group::PersonalWeapon)
    } else if COMPONENT.contains(&item_type) || WEAPON_ENDINGS.iter().any(|end| item_type.ends_with(end)) {
        Some(Group::Component)
    } else {
        None
    }
}

/// The `Item Type:` (or `Type:`) of a description: the text behind it, up to the end of its line (the texts of the
/// game write a line break as the two characters `\n`).
pub fn item_type_of(description: &str) -> Option<&str> {
    description.split("\\n").map(str::trim).find_map(|line| {
        let value = line.strip_prefix("Item Type:").or_else(|| line.strip_prefix("Type:"))?.trim();
        (!value.is_empty()).then_some(value)
    })
}

/// A name of the game as the scan compares it: no line break written out, no fixed space; `None` where the text is
/// no name (a placeholder, a text key, a mark of the developers).
fn clean_name(text: &str) -> Option<String> {
    let name = text.replace("\\n", " ").replace('\u{a0}', " ");
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let low = name.to_lowercase();
    let bad = name.len() < 2
        || name.starts_with(['@', '<', '['])
        || low.contains("placeholder")
        || low.contains("loc_")
        || low.starts_with("item_")
        || low.starts_with("test");
    (!bad).then_some(name)
}

impl GameTexts {
    /// Every item whose description says it is one of the kinds the scan needs (see [`group_of_type`]), by its name
    /// text.
    pub fn named_items(&self) -> Vec<(String, Group)> {
        let mut found = Vec::new();
        for (low, (_, text)) in &self.by_key {
            let Some(ending) = low.strip_prefix("item_name") else { continue };
            let Some((_, description)) = self.by_key.get(&format!("item_desc{ending}")) else { continue };
            let Some(group) = item_type_of(description).and_then(group_of_type) else { continue };
            if let Some(name) = clean_name(text) {
                found.push((name, group));
            }
        }
        found.sort();
        found.dedup();
        found
    }
}

/// The names of the kinds of items the scan needs, with the group of each: every blueprint of the catalog and every
/// item of the English texts of the game that is a component, armour or a weapon of a person. A name both a blueprint
/// and an item of the texts counts as a blueprint.
pub fn important_names(
    texts: Option<&GameTexts>,
    catalog: Option<&crate::catalog_cache::CatalogCache>,
) -> BTreeMap<String, Group> {
    let mut all = BTreeMap::new();
    for (name, group) in texts.map(GameTexts::named_items).unwrap_or_default() {
        all.entry(name).or_insert(group);
    }
    for blueprint in catalog.map(|c| c.blueprints.as_slice()).unwrap_or_default() {
        all.insert(blueprint.name.clone(), Group::Blueprint);
    }
    all
}

/// Adds the names of [`important_names`] the comparison set lacks, as names without a tag; how many were added.
/// A name the set already holds in another spelling (`(100 Cap)` for `(100 cap)`) is left out: the stock books under
/// its own spelling, so that one must stay the only one.
pub fn add_important_names(names: &mut Names, important: &BTreeMap<String, Group>) -> usize {
    let mut known: std::collections::HashSet<String> = names.all().map(crate::blueprint_scan::plain).collect();
    let before = names.len();
    for name in important.keys() {
        if known.insert(crate::blueprint_scan::plain(name)) {
            names.add(Entry { name: name.clone(), ..Entry::default() });
        }
    }
    names.len() - before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_line_gives_the_kind() {
        let description = "Manufacturer: ArmaMod\\nType: Suppressor\\nAttachment Point: Barrel\\nSize: 1";
        assert_eq!(item_type_of(description), Some("Suppressor"));
        assert_eq!(
            item_type_of("Manufacturer: Gemini \\nItem Type: Sniper Rifle\\nClass: Ballistic"),
            Some("Sniper Rifle")
        );
        assert_eq!(item_type_of("A livery for the Cutlass."), None);
    }

    #[test]
    fn only_the_kinds_the_scan_needs_have_a_group() {
        assert_eq!(group_of_type("Heavy Armor"), Some(Group::Armor));
        assert_eq!(group_of_type("Undersuit"), Some(Group::Armor));
        assert_eq!(group_of_type("Assault Rifle"), Some(Group::PersonalWeapon));
        assert_eq!(group_of_type("Magazine"), Some(Group::PersonalWeapon));
        assert_eq!(group_of_type("Telescopic"), Some(Group::PersonalWeapon));
        assert_eq!(group_of_type("Cooler"), Some(Group::Component));
        assert_eq!(group_of_type("Laser Repeater"), Some(Group::Component));
        assert_eq!(group_of_type("Ballistic Gatling Turret"), Some(Group::Component));
        for left_out in ["Flight Suit", "Medical Consumable", "Worksuit", "Container (Medical)", "Storage", ""] {
            assert_eq!(group_of_type(left_out), None, "{left_out}");
        }
    }

    #[test]
    fn items_come_by_name_and_description() {
        let texts = GameTexts::parse(
            "item_NameStoic_Supp=Stoic Suppressor1\n\
             item_DescStoic_Supp=Manufacturer: ArmaMod\\nType: Suppressor\\nSize: 1\n\
             item_NameCryo=Cryopod (Vanduul)\n\
             item_DescCryo=A pod.\n\
             item_NamePaint=Cutlass Akuma Livery\n\
             item_DescPaint=Item Type: Paint\\nA livery.\n\
             item_Name_arms,P=Venture Arms\u{a0}Red\n\
             item_Desc_arms,P=Item Type: Light Armor\\nArms.\n\
             item_NameNoDesc=Lonely Name\n",
        );
        let found = texts.named_items();
        assert_eq!(
            found,
            vec![
                ("Stoic Suppressor1".to_string(), Group::PersonalWeapon),
                ("Venture Arms Red".to_string(), Group::Armor),
            ]
        );
    }

    #[test]
    fn a_blueprint_stays_a_blueprint_and_names_are_added_once() {
        let mut important = BTreeMap::new();
        important.insert("Stoic Suppressor1".to_string(), Group::PersonalWeapon);
        let mut names = Names::new();
        assert_eq!(add_important_names(&mut names, &important), 1);
        assert_eq!(add_important_names(&mut names, &important), 0);
        assert!(names.entry("Stoic Suppressor1").is_some());
    }

    #[test]
    fn a_spelling_the_set_has_in_other_case_is_not_added() {
        let mut names = Names::new();
        names.add(Entry { name: "Demeco LMG Battery (100 Cap)".to_string(), ..Entry::default() });
        let mut important = BTreeMap::new();
        important.insert("Demeco LMG Battery (100 cap)".to_string(), Group::PersonalWeapon);
        assert_eq!(add_important_names(&mut names, &important), 0);
        assert!(names.entry("Demeco LMG Battery (100 cap)").is_none());
    }

    #[test]
    fn placeholders_are_no_names() {
        assert_eq!(clean_name("Venture\u{a0}Arms\\n"), Some("Venture Arms".to_string()));
        for bad in ["", "@item_Name_Foo", "<= PLACEHOLDER =>", "item_Name_Foo", "[PH] Rifle", "Test Rifle"] {
            assert_eq!(clean_name(bad), None, "{bad}");
        }
    }
}
