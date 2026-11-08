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

//! Which parts can go into a slot (`cart.choices` of 4.12): the parts that
//! can be bought (the UEX catalog), the parts that can be built (the recipes
//! with the traits of the blueprint catalog) and the parts only the game data
//! of erkul knows (pledge packages, events).
//!
//! ⚠⚠ Two sources because none is enough alone: UEX knows what stands in a
//! shop (no military parts), the recipes know what can be built. Merged by the
//! entity identifier, never by the name, except the one case marked below.
//!
//! ⚠⚠ A closed list, no free text: only what a source knows is offered. If no
//! source knows the kind, the list is empty and the page says so instead of
//! offering half the catalog; a cooler in a weapon slot is worse than no choice.
//!
//! ⚠ The table of UEX groups ([`GROUP_TO_KIND`]) only filters the list, it
//! never decides an assignment; that goes by the identifier alone.

use std::collections::{HashMap, HashSet};

use crate::erkul_slots::{ErkulParts, SlotInfo, fits_slot};

/// Where a part can be had from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Buyable,
    Craftable,
    Both,
    /// Fits according to the game data, but neither in a shop nor among the blueprints.
    ShipOnly,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Buyable => "kaufbar",
            Origin::Craftable => "herstellbar",
            Origin::Both => "beides",
            Origin::ShipOnly => "schiffsteil",
        }
    }
}

/// One part of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub name: String,
    pub reference: String,
    pub maker: String,
    pub grade: String,
    pub class: String,
    pub origin: Origin,
    /// The size of the part where a source knows it.
    pub size: Option<i64>,
}

/// UEX group → kind of slot (`cart.GROUP_TO_KIND`).
pub const GROUP_TO_KIND: [(&str, &str); 23] = [
    ("Coolers", "Cooler"),
    ("Power Plants", "PowerPlant"),
    ("Shield Generators", "Shield"),
    ("Quantum Drives", "QuantumDrive"),
    ("Radar", "Radar"),
    ("Jump Modules", "JumpDrive"),
    ("Guns", "WeaponGun"),
    ("Turrets", "Turret"),
    ("Point Defense Cannon", "Turret"),
    ("Missiles", "Missile"),
    ("Missile Racks", "MissileLauncher"),
    ("Torpedo Tubes", "MissileLauncher"),
    ("Bombs", "Bomb"),
    ("Bomb Racks", "BombLauncher"),
    ("Mining Laser Heads", "WeaponMining"),
    ("Mining Modules", "MiningModifier"),
    ("Salvage Beams", "SalvageHead"),
    ("Scraper Beams", "SalvageHead"),
    ("Tractor Beams", "TractorBeam"),
    ("Batteries", "Battery"),
    ("Container", "Container"),
    ("Flight Blade", "FlightController"),
    ("Gravity Generator", "GravityGenerator"),
];

/// Recipe kind → kind of slot, the fallback where the catalog is silent (`cart.RECIPE_TO_KIND`).
const RECIPE_TO_KIND: [(&str, &str); 7] = [
    ("quantumdrive", "QuantumDrive"),
    ("cooler", "Cooler"),
    ("powerplant", "PowerPlant"),
    ("shield", "Shield"),
    ("radar", "Radar"),
    ("mininglaser", "WeaponMining"),
    ("tractorbeam", "TractorBeam"),
];

/// The kinds whose parts hang on tags and are chosen by the rule of the slot (`erkul.PART_KINDS`).
const PART_KINDS: [&str; 6] = ["MissileLauncher", "BombLauncher", "Missile", "Bomb", "Paints", "FlightController"];

/// What the blueprint catalog knows about a made item (`catalog['bauplaene'][key]`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Traits {
    pub kind: String,
    pub size: Option<i64>,
    pub maker: String,
    /// The grade as a number, 1 = A.
    pub grade: Option<i64>,
    pub class: String,
}

/// An item that can be crafted (`crafting.all_items()`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftEntry {
    /// The entity identifier of the product, lower case; empty without one.
    pub entity: String,
    /// The name of the product.
    pub basis: String,
    /// The name as the list shows it.
    pub name: String,
    pub maker: String,
    /// The kind the recipe folder names (`cooler`, `weapons` …).
    pub recipe_kind: String,
    /// The subkind (`size2`, `laser` …).
    pub recipe_subtype: String,
}

/// A part a shop sells (`shops.catalog_items()`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShopPart {
    /// The entity identifier or `id:<number>`.
    pub code: String,
    pub name: String,
    /// The group as UEX names it.
    pub group: String,
    pub maker: String,
    pub size: String,
    pub class: String,
    pub grade: String,
}

/// The sources of the list.
pub struct Sources<'a> {
    pub craft: &'a [CraftEntry],
    /// By the comparable name of the product (`paths.name_key` of the basis).
    pub traits: &'a HashMap<String, Traits>,
    pub shop: &'a [ShopPart],
    pub parts: &'a ErkulParts,
}

/// `cart._grade_letter`: `2` becomes `B`, everything else goes through as it is.
fn grade_letter(value: &str) -> String {
    let text = value.trim();
    match text {
        "1" => "A",
        "2" => "B",
        "3" => "C",
        "4" => "D",
        other => other,
    }
    .to_string()
}

/// `cart._from_recipe`: kind and size out of the recipe data.
fn from_recipe(entry: &CraftEntry) -> (Option<&'static str>, Option<i64>) {
    let wanted = entry.recipe_kind.to_lowercase();
    let Some(kind) = RECIPE_TO_KIND.iter().find(|(k, _)| *k == wanted).map(|(_, v)| *v) else { return (None, None) };
    let size = entry
        .recipe_subtype
        .to_lowercase()
        .strip_prefix("size")
        .filter(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
        .and_then(|rest| rest.parse::<i64>().ok());
    (Some(kind), size)
}

/// `cart._craftable`: all items of this kind and size that can be crafted, identifier → entry, in the
/// order of the recipes.
///
/// ⚠ Linked by the name, rightly this once: recipes and catalog come from the same source and use the
/// same form of the name. The identifier stays the key of the result.
fn craftable(sources: &Sources, kind: &str, size: Option<i64>) -> Vec<Choice> {
    let mut result: Vec<Choice> = Vec::new();
    for entry in sources.craft {
        if entry.entity.is_empty() {
            continue;
        }
        let traits = sources.traits.get(&crate::paths::name_key(&entry.basis));
        let mut own_kind = traits.map(|t| t.kind.clone()).unwrap_or_default();
        let mut own = traits.and_then(|t| t.size);
        if own_kind.is_empty() {
            let (from_kind, from_size) = from_recipe(entry);
            own_kind = from_kind.unwrap_or("").to_string();
            if own.is_none() {
                own = from_size;
            }
        }
        if own_kind != kind {
            continue;
        }
        // The size only where both sides have one: a part must not fall out because a value is missing.
        if let (Some(wanted), Some(own)) = (size, own)
            && own != wanted
        {
            continue;
        }
        let choice = Choice {
            name: if entry.basis.is_empty() { entry.name.clone() } else { entry.basis.clone() },
            reference: entry.entity.clone(),
            maker: traits.map(|t| t.maker.clone()).filter(|m| !m.is_empty()).unwrap_or_else(|| entry.maker.clone()),
            // `str(value or '')`: a grade of 0 is none.
            grade: traits
                .and_then(|t| t.grade)
                .filter(|g| *g != 0)
                .map(|g| grade_letter(&g.to_string()))
                .unwrap_or_default(),
            class: traits.map(|t| t.class.clone()).unwrap_or_default(),
            origin: Origin::Craftable,
            size: own,
        };
        match result.iter_mut().find(|c| c.reference == entry.entity) {
            Some(existing) => *existing = choice,
            None => result.push(choice),
        }
    }
    result
}

/// `cart._ship_only_parts`: parts of the game data that neither shop nor blueprint knows. With tags at
/// the slot `fits_slot` decides, else kind and size.
fn ship_only(
    sources: &Sources,
    kind: &str,
    size: Option<i64>,
    slot: Option<&SlotInfo>,
    known: &[Choice],
) -> Vec<Choice> {
    let have_ids: HashSet<&str> = known.iter().map(|k| k.reference.as_str()).collect();
    let mut have_names: HashSet<String> = known.iter().map(|k| k.name.trim().to_lowercase()).collect();
    let fit = slot.and_then(|s| s.fit.as_ref());
    let mut result = Vec::new();
    for part in sources.parts.all() {
        if part.kind != kind {
            continue;
        }
        if let Some(fit) = fit {
            if !fits_slot(fit, part) {
                continue;
            }
        } else if let (Some(wanted), Some(own)) = (size, part.size)
            && own != wanted
        {
            continue;
        }
        let name = part.name.trim();
        if name.is_empty() || have_ids.contains(part.reference.as_str()) || have_names.contains(&name.to_lowercase()) {
            continue;
        }
        have_names.insert(name.to_lowercase());
        result.push(Choice {
            name: name.to_string(),
            reference: part.reference.clone(),
            maker: String::new(),
            grade: part.grade.clone(),
            class: String::new(),
            origin: Origin::ShipOnly,
            size: part.size,
        });
    }
    result
}

/// `cart._choices_by_rule`: the list for racks, missiles, bombs and paints. Here the slot decides, not
/// the shop: the torpedo slot of the Eclipse takes exactly four racks; by kind and size alone 126 would
/// stand to choose, almost all wrong. Shop and blueprint only add where the part comes from.
///
/// ⚠⚠ Variants of the same name become one: the Cutlass Black has "MSD-481 Missile Rack" four times,
/// with four identifiers, and a click on one of them would pick a random one. Kept is the identifier
/// that is fitted at the factory, else the one from a shop, else the one from a blueprint.
fn by_rule(sources: &Sources, kind: &str, slot: &SlotInfo) -> Vec<Choice> {
    let craft = craftable(sources, kind, None);
    let shop: HashMap<&str, &ShopPart> =
        sources.shop.iter().filter(|p| !p.code.is_empty()).map(|p| (p.code.as_str(), p)).collect();
    // UEX has wares without an entity identifier, only with `id:…` (the flight blades): found by the full name,
    // where that name stands once. "Flight Blade" is the name of many factory blades, each of another ship.
    let mut shop_by_name: HashMap<String, Option<&ShopPart>> = HashMap::new();
    for part in sources.shop.iter().filter(|p| p.code.starts_with("id:")) {
        shop_by_name.entry(part.name.trim().to_lowercase()).and_modify(|seen| *seen = None).or_insert(Some(part));
    }
    let stock = slot.factory.as_ref().map(|(reference, _)| reference.as_str());
    let in_craft = |reference: &str| craft.iter().find(|c| c.reference == reference);

    let mut fitting = sources.parts.compatible(slot);
    fitting.sort_by(|a, b| {
        let rank = |p: &&crate::erkul_slots::Part| {
            let reference = p.reference.as_str();
            (
                Some(reference) != stock,
                !shop.contains_key(reference),
                in_craft(reference).is_none(),
                reference.to_string(),
            )
        };
        rank(a).cmp(&rank(b))
    });
    let mut best: Vec<(String, &crate::erkul_slots::Part)> = Vec::new();
    for part in fitting {
        let key = part.name.trim().to_lowercase();
        if !best.iter().any(|(k, _)| *k == key) {
            best.push((key, part));
        }
    }

    let mut result = Vec::new();
    for (_, part) in best {
        let mut reference = part.reference.clone();
        let mut shop_hit: Option<&ShopPart> = shop.get(reference.as_str()).copied();
        let craft_hit = in_craft(&reference);
        // The factory part keeps its own identifier: a ware of the same name may be the blade of another ship.
        if shop_hit.is_none()
            && Some(part.reference.as_str()) != stock
            && let Some(Some(found)) = shop_by_name.get(&part.name.trim().to_lowercase())
        {
            // The identifier of the shop, so that price and shop are found.
            reference.clone_from(&found.code);
            shop_hit = Some(found);
        }
        let origin = match (shop_hit.is_some(), craft_hit.is_some()) {
            (true, true) => Origin::Both,
            (true, false) => Origin::Buyable,
            (false, true) => Origin::Craftable,
            (false, false) => Origin::ShipOnly,
        };
        // `known = in_shop or in_craft`: the shop's entry wins where there is one.
        let (known_name, maker, grade, class) = match (shop_hit, craft_hit) {
            (Some(p), _) => (p.name.clone(), p.maker.clone(), p.grade.clone(), p.class.clone()),
            (None, Some(c)) => (c.name.clone(), c.maker.clone(), c.grade.clone(), c.class.clone()),
            (None, None) => (String::new(), String::new(), String::new(), String::new()),
        };
        result.push(Choice {
            name: if part.name.is_empty() { known_name } else { part.name.clone() },
            reference,
            maker,
            grade,
            class,
            origin,
            size: part.size,
        });
    }
    result.sort_by_cached_key(|c| c.name.to_lowercase());
    result
}

/// `cart.choices`: which parts go into a slot of this kind and size, alphabetical. With `slot` the rule of
/// the game replaces kind and size for racks, missiles, bombs and paints, and for a ship-bound slot
/// ([`crate::erkul_slots::SHIP_BOUND`]) the factory part does where the slot has no rule of its own.
///
/// ⚠⚠ A flight blade belongs to one ship: by kind and size alone the blade slot of the F8C Lightning offered
/// the blades of every ship (the shop lists them all as "Flight Blade", size 1).
pub fn choices(sources: &Sources, kind: &str, size: Option<i64>, slot: Option<&SlotInfo>) -> Vec<Choice> {
    if kind.is_empty() {
        return Vec::new();
    }
    if let Some(slot) = slot
        && PART_KINDS.contains(&kind)
        && !sources.parts.is_empty()
        && (slot.fit.is_some() || sources.parts.bound_stock(slot).is_some())
    {
        return by_rule(sources, kind, slot);
    }
    // A ship-bound slot whose factory part is not known has no rule to go by: no list rather than the parts of
    // every ship (the page still offers the factory part). A paint port empty at the factory, of a ship neither
    // the Record nor erkul gives the rule of, is one.
    if crate::erkul_slots::SHIP_BOUND.contains(&kind) && slot.is_some_and(|s| s.fit.is_none()) {
        return Vec::new();
    }
    let found = craftable(sources, kind, size);
    let groups: Vec<&str> = GROUP_TO_KIND.iter().filter(|(_, k)| *k == kind).map(|(g, _)| *g).collect();
    let mut result: Vec<Choice> = Vec::new();
    let mut found = found;
    if !groups.is_empty() {
        for part in sources.shop {
            if !groups.contains(&part.group.as_str()) {
                continue;
            }
            // The size only where both sides have one: UEX leaves the field empty for a part of the wares.
            let own = part.size.trim();
            if let (Some(wanted), false) = (size, own.is_empty())
                && let Ok(number) = own.parse::<f64>()
                && number.trunc() as i64 != wanted
            {
                continue;
            }
            if let Some(already) = found.iter_mut().find(|c| c.reference == part.code) {
                already.origin = Origin::Both;
                if already.size.is_none() {
                    already.size = crate::size_order::parse(&part.size);
                }
                // UEX looks after class and grade of its wares more carefully: where it has a value, it wins.
                for (field, value) in [
                    (&mut already.class, &part.class),
                    (&mut already.grade, &part.grade),
                    (&mut already.maker, &part.maker),
                ] {
                    if !value.is_empty() {
                        field.clone_from(value);
                    }
                }
                continue;
            }
            result.push(Choice {
                name: part.name.clone(),
                reference: part.code.clone(),
                maker: part.maker.clone(),
                grade: part.grade.clone(),
                class: part.class.clone(),
                origin: Origin::Buyable,
                size: crate::size_order::parse(&part.size),
            });
        }
    }
    result.extend(found);
    let extra = ship_only(sources, kind, size, slot, &result);
    result.extend(extra);
    result.sort_by_cached_key(|c| c.name.to_lowercase());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::erkul_slots::Part;

    fn blade(reference: &str, name: &str, class: &str, tags: &[&str]) -> Part {
        Part {
            reference: reference.into(),
            name: name.into(),
            kind: "FlightController".into(),
            size: Some(1),
            tags: tags.iter().map(|s| (*s).to_string()).collect(),
            class: class.into(),
            ..Part::default()
        }
    }

    fn shop(code: &str, name: &str) -> ShopPart {
        ShopPart { code: code.into(), name: name.into(), group: "Flight Blade".into(), size: "1".into(), ..ShopPart::default() }
    }

    fn slot(factory: Option<(&str, &str)>) -> SlotInfo {
        SlotInfo {
            path: "hardpoint_controller_flight".into(),
            kind: "FlightController".into(),
            size: Some(1),
            fit: None,
            factory: factory.map(|(r, n)| (r.to_string(), n.to_string())),
            fixed: false,
        }
    }

    fn names(found: &[Choice]) -> Vec<&str> {
        found.iter().map(|c| c.name.as_str()).collect()
    }

    /// The F8C Lightning in the hangar: its blade slot sits right on the ship (no rule of its own in the game
    /// data), and the shop lists the blades of every ship as "Flight Blade" of size 1.
    #[test]
    fn the_blade_slot_of_a_ship_offers_its_own_blades_and_not_those_of_every_ship() {
        crate::language::set_thread_language(crate::language::Choice::De);
        let parts = ErkulParts::from_parts(vec![
            blade("f8c", "Flight Blade", "Controller_Flight_ANVL_Lightning_F8C", &["ANVL_Lighting_Blade"]),
            blade("f8", "Flight Blade", "Controller_Flight_ANVL_Lightning_F8", &["ANVL_Lighting_Blade"]),
            blade("glad", "Gladius Standard Flight Blade", "Controller_Flight_AEGS_Gladius", &["AEGS_Gladius_Blade"]),
            blade("glad-hnd", "Gladius PHB Flight Blade", "Controller_Flight_AEGS_Gladius_Blade_HND", &["AEGS_Gladius_Blade"]),
            blade("asgard", "Anvil Asgard Standard Flight Blade", "Controller_Flight_ANVL_Asgard", &["ANVL_Asgard_Blade"]),
            blade("stalker", "Avenger Stalker PHB Flight Blade", "Controller_Flight_AEGS_Avenger_Stalker_Flight_Blade_HND", &["AEGS_Avenger_Blade"]),
        ]);
        let wares = [
            shop("id:1", "Anvil Asgard Standard Flight Blade"),
            shop("id:2", "Arrow Standard Flight Blade"),
            shop("id:3", "Gladius PHB Flight Blade"),
            shop("id:4", "Avenger Stalker PHB Flight Blade"),
            // Two wares of the factory name of many ships: neither gives its identifier to the F8C's blade.
            shop("id:5", "Flight Blade"),
            shop("id:6", "Flight Blade"),
        ];
        let traits = HashMap::new();
        let sources = Sources { craft: &[], traits: &traits, shop: &wares, parts: &parts };
        // The F8C: only its factory blade (the F8's has the same name and becomes one with it).
        let f8c = choices(&sources, "FlightController", Some(1), Some(&slot(Some(("f8c", "Flight Blade")))));
        assert_eq!(names(&f8c), ["Flight Blade"]);
        assert_eq!(f8c[0].reference, "f8c", "the factory blade wins among those of the same name");
        assert_eq!(f8c[0].origin, Origin::ShipOnly, "no shop ware of the same name lends it price and shop");
        // The Gladius: its own two, the one in the shop with the shop's identifier and origin.
        let gladius =
            choices(&sources, "FlightController", Some(1), Some(&slot(Some(("glad", "Gladius Standard Flight Blade")))));
        assert_eq!(names(&gladius), ["Gladius PHB Flight Blade", "Gladius Standard Flight Blade"]);
        let bought = gladius.iter().find(|c| c.name == "Gladius PHB Flight Blade").expect("in the list");
        assert_eq!((bought.reference.as_str(), bought.origin), ("id:3", Origin::Buyable));
        // Counter-check: without the slot (no ship known) kind and size decide, and every ship's blade stands there.
        let loose = choices(&sources, "FlightController", Some(1), None);
        assert!(names(&loose).contains(&"Arrow Standard Flight Blade"), "{:?}", names(&loose));
        // A slot whose factory blade is unknown gets no list instead of every ship's blades.
        assert!(choices(&sources, "FlightController", Some(1), Some(&slot(Some(("lost", "Flight Blade"))))).is_empty());
        assert!(choices(&sources, "FlightController", Some(1), Some(&slot(None))).is_empty());
        // Other kinds without a rule of their own keep kind and size.
        let cooler = SlotInfo { kind: "Cooler".into(), ..slot(None) };
        let coolers = [ShopPart { group: "Coolers".into(), ..shop("id:9", "Bracer") }];
        let sources = Sources { shop: &coolers, ..sources };
        assert_eq!(names(&choices(&sources, "Cooler", Some(1), Some(&cooler))), ["Bracer"]);
    }

    /// The paint slot of a ship sits right on the ship and is empty at the factory: without a rule of its own the
    /// list held every paint of the game (1064 at 4.10.2).
    #[test]
    fn the_paint_slot_offers_the_paints_of_its_ship_and_not_those_of_every_ship() {
        crate::language::set_thread_language(crate::language::Choice::De);
        let paint = |reference: &str, name: &str, needs: &str| Part {
            reference: reference.into(),
            name: name.into(),
            kind: "Paints".into(),
            size: Some(1),
            tags: vec![needs.to_string(), format!("@{reference}")],
            required: vec![needs.to_string()],
            ..Part::default()
        };
        let parts = ErkulParts::from_parts(vec![
            paint("pirate", "Gladius Pirate Livery", "Paint_Gladius"),
            paint("valiant", "Gladius Valiant Livery", "Paint_Gladius"),
            paint("platinum", "Lightning Platinum Livery", "Paint_Lightning"),
            paint("cutter", "Cutter Rust Livery", "Paint_Cutter"),
        ]);
        let traits = HashMap::new();
        let sources = Sources { craft: &[], traits: &traits, shop: &[], parts: &parts };
        let slot = |factory: Option<(&str, &str)>, fit: Option<crate::erkul_slots::Fit>| SlotInfo {
            path: "hardpoint_paint".into(),
            kind: "Paints".into(),
            size: None,
            fit,
            factory: factory.map(|(r, n)| (r.to_string(), n.to_string())),
            fixed: false,
        };
        // Empty at the factory, no rule: no list.
        assert!(choices(&sources, "Paints", None, Some(&slot(None, None))).is_empty());
        // With the factory paint of a Gladius: the Gladius paints.
        let gladius = choices(&sources, "Paints", None, Some(&slot(Some(("pirate", "Gladius Pirate Livery")), None)));
        assert_eq!(names(&gladius), ["Gladius Pirate Livery", "Gladius Valiant Livery"]);
        // With the rule of the port (erkul's or the ship Record's): the paints of that family.
        let lightning = vec!["Paint_Lightning".to_string()];
        let fit = crate::erkul_slots::Fit {
            types: vec![("Paints".into(), Vec::new())],
            port_tags: lightning.clone(),
            required: lightning,
            ..Default::default()
        };
        assert_eq!(names(&choices(&sources, "Paints", None, Some(&slot(None, Some(fit))))), ["Lightning Platinum Livery"]);
        // Counter-check: without the slot every paint stands there.
        assert_eq!(choices(&sources, "Paints", None, None).len(), 4);
    }
}
