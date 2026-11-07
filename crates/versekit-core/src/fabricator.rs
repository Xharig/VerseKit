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

//! The categories of the crafting terminal in the game (ten, each with its
//! subcategories) and where a craftable item stands in them.
//!
//! This is the one place that knows the tree and the way an item is sorted
//! into it. The item is described by three facts the recipe data carries:
//! the item type of the product (`Char_Armor_Helmet`, `WeaponPersonal` …),
//! the kind the folder of the blueprint record names (`armour`, `ammo` …)
//! and its subkind (`pistol`, `combat` …). [`RULES`] turns them into a
//! [`Place`]; every rule names one place, and the rules are written so that
//! at most one fits an item (the tests check it on the kinds of a real build).
//! Once the blueprint records are read with their own category, [`place`]
//! is the function to feed with it instead.
//!
//! The names come from the game where it has them: the filter texts of the
//! inventory (`ui_inventory_filter_category_name_<game>` in the `global.ini`
//! of the game language), which the terminal shows. [`Names`] holds them;
//! where the game has no text, or one it uses for another entry too, the own
//! text (`s_fb_kat_<key>`, `s_fb_un_<key>`) stands in.

use std::collections::HashMap;

use crate::language::{self, Lang};

/// One subcategory: its key and the key of its name in the game, if it has one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sub {
    pub key: &'static str,
    pub game: Option<&'static str>,
}

/// One category of the terminal: its key, the key of its name in the game,
/// the Lucide icon of its button and its subcategories in the order of the game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Category {
    pub key: &'static str,
    pub game: Option<&'static str>,
    pub icon: &'static str,
    pub subs: &'static [Sub],
}

const fn s(key: &'static str, game: &'static str) -> Sub {
    Sub { key, game: Some(game) }
}

/// The ten categories in the order of the buttons of the terminal.
pub const CATEGORIES: [Category; 10] = [
    Category {
        key: "panzerung",
        game: Some("armor"),
        icon: "shield-half",
        subs: &[
            s("undersuits", "undersuits"),
            s("helme", "armor_helmets"),
            s("brust", "armor_core"),
            s("arme", "armor_arms"),
            s("beine", "armor_legs"),
            s("rucksaecke", "armor_backpack"),
            s("flair", "armor_flair"),
        ],
    },
    Category {
        key: "kleidung",
        game: Some("clothing"),
        icon: "shirt",
        subs: &[
            s("kopfbedeckung", "clothing_hats"),
            s("hemden", "clothing_torso0"),
            s("jacken", "clothing_torso1"),
            s("handschuhe", "clothing_hands"),
            s("beinbekleidung", "clothing_legs"),
            s("schuhwerk", "clothing_feet"),
            s("brille", "clothing_eyewear"),
        ],
    },
    Category {
        key: "waffen",
        game: Some("weapons"),
        icon: "crosshair",
        subs: &[
            s("seitenwaffen", "pistols"),
            s("primaer", "stocked"),
            s("spezial", "shouldered"),
            s("nahkampf", "melee"),
            s("aufsaetze", "weaponattachments"),
            s("wurfobjekte", "throwables"),
        ],
    },
    Category {
        key: "utility",
        game: Some("utility"),
        icon: "wrench",
        subs: &[
            s("gadgets", "gadgets"),
            s("gadget_aufsaetze", "gadgetattachments"),
            s("medizinisch", "consumables_medical"),
            s("kryptoschluessel", "consumables_chips"),
            s("technologie", "technology"),
        ],
    },
    Category { key: "munition", game: Some("ammo"), icon: "target", subs: &[] },
    Category {
        key: "vehikel",
        game: Some("vehicle_parts"),
        icon: "rocket",
        subs: &[
            s("artillerie", "vehicle_ordnance"),
            s("kuehler", "vehicle_cooler"),
            s("emp", "vehicle_emp"),
            s("bergbau", "vehicle_mining"),
            s("raketenhalterungen", "vehicle_missilerack"),
            s("kraftwerke", "vehicle_powerplant"),
            s("quantum_drives", "vehicle_quantumdrive"),
            s("schilde", "vehicle_shield"),
            s("geschuetztuerme", "vehicle_turret"),
            s("waffen", "vehicle_weapon"),
            s("gegenmassnahmen", "vehicle_countermeasure"),
            s("lackierungen", "vehicle_paint"),
            s("sprungmodule", "vehicle_jumpdrive"),
            s("radar", "vehicle_radar"),
        ],
    },
    Category {
        key: "nahrung",
        game: Some("sustenance"),
        icon: "hamburger",
        subs: &[s("lebensmittel", "food"), s("getraenke", "drink")],
    },
    Category {
        key: "behaelter",
        game: Some("containers"),
        icon: "container",
        subs: &[s("waren_cargo", "cargo"), Sub { key: "behaelter", game: None }],
    },
    Category { key: "sonstiges", game: Some("other"), icon: "ellipsis", subs: &[] },
    // The game has no text of its own for this button.
    Category { key: "mission", game: None, icon: "circle-alert", subs: &[] },
];

/// The index of the category everything without a rule lands in.
pub const MISC: usize = 8;

/// The start of the keys of the filter texts in the `global.ini`.
pub const GAME_KEY: &str = "ui_inventory_filter_category_name_";

/// The key of the game for "all categories".
const GAME_ALL: &str = "all_categories";

/// The names of the game for the tree, per language: key (without
/// [`GAME_KEY`]) to text. Empty: the own texts stand everywhere.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Names {
    pub de: HashMap<String, String>,
    pub en: HashMap<String, String>,
}

impl Names {
    /// From the `ui_…` texts of a `global.ini` per language (as
    /// [`crate::controls::ini_texts`] reads them): only the filter texts are kept.
    pub fn from_ini(de: &HashMap<String, String>, en: &HashMap<String, String>) -> Names {
        let keep = |all: &HashMap<String, String>| {
            all.iter()
                .filter_map(|(k, v)| k.strip_prefix(GAME_KEY).map(|k| (k.to_string(), v.clone())))
                .filter(|(_, v)| !v.trim().is_empty())
                .collect()
        };
        Names { de: keep(de), en: keep(en) }
    }

    fn of(&self, lang: Lang) -> &HashMap<String, String> {
        match lang {
            Lang::De => &self.de,
            Lang::En => &self.en,
        }
    }

    /// The text of the game for the category key `game`, unless it is empty or
    /// the game writes the same text for a subcategory (some translations give
    /// a category the name of a subcategory, "Containers" as "Liveries").
    fn game_category(&self, lang: Lang, game: Option<&str>) -> Option<String> {
        let texts = self.of(lang);
        let text = texts.get(game?)?;
        let taken = CATEGORIES
            .iter()
            .flat_map(|c| c.subs)
            .any(|sub| sub.game.is_some_and(|g| texts.get(g).is_some_and(|t| t == text)));
        (!taken).then(|| text.clone())
    }

    /// How a category is called in `lang`.
    pub fn category_in(&self, lang: Lang, index: usize) -> String {
        let Some(category) = CATEGORIES.get(index) else { return String::new() };
        self.game_category(lang, category.game)
            .unwrap_or_else(|| language::t_in(lang, &format!("s_fb_kat_{}", category.key)))
    }

    /// How a subcategory is called in `lang`.
    pub fn sub_in(&self, lang: Lang, key: &str) -> String {
        let game = CATEGORIES.iter().flat_map(|c| c.subs).find(|s| s.key == key).and_then(|s| s.game);
        game.and_then(|g| self.of(lang).get(g).cloned())
            .unwrap_or_else(|| language::t_in(lang, &format!("s_fb_un_{key}")))
    }

    /// How a category is called in the language of the window.
    pub fn category(&self, index: usize) -> String {
        self.category_in(language::current(), index)
    }

    /// How a subcategory is called in the language of the window.
    pub fn sub(&self, key: &str) -> String {
        self.sub_in(language::current(), key)
    }

    /// "All categories", the first entry of a menu.
    pub fn all(&self) -> String {
        let lang = language::current();
        self.of(lang).get(GAME_ALL).cloned().unwrap_or_else(|| language::t_in(lang, "s_fb_alle_kat"))
    }

    /// The path of a place in `lang`: "Waffen  ·  Primär", or only the category.
    pub fn path_in(&self, lang: Lang, place: Place) -> String {
        match place.sub {
            Some(sub) => format!("{}  ·  {}", self.category_in(lang, place.category), self.sub_in(lang, sub)),
            None => self.category_in(lang, place.category),
        }
    }

    /// The path of a place in the language of the window.
    pub fn path(&self, place: Place) -> String {
        self.path_in(language::current(), place)
    }
}

/// Where an item stands: the index into [`CATEGORIES`] and the subcategory,
/// `None` where the category has none for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Place {
    pub category: usize,
    pub sub: Option<&'static str>,
}

impl Place {
    /// The position of the subcategory within its category, for sorting:
    /// without a subcategory after all others.
    pub fn sub_order(&self) -> usize {
        let subs = CATEGORIES[self.category].subs;
        self.sub.and_then(|s| subs.iter().position(|x| x.key == s)).unwrap_or(subs.len())
    }
}

/// One rule: the item type (or, without one, the kind of the folder) must be
/// one of `kinds`; `recipe` and `subtypes` narrow it further where given.
/// All compared in lower case.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub kinds: &'static [&'static str],
    pub recipe: Option<&'static [&'static str]>,
    pub subtypes: Option<&'static [&'static str]>,
    pub category: &'static str,
    pub sub: Option<&'static str>,
}

const fn rule(kinds: &'static [&'static str], category: &'static str, sub: Option<&'static str>) -> Rule {
    Rule { kinds, recipe: None, subtypes: None, category, sub }
}

/// The rules from the facts of an item to its place.
pub const RULES: [Rule; 29] = [
    rule(&["char_armor_undersuit"], "panzerung", Some("undersuits")),
    rule(&["char_armor_helmet"], "panzerung", Some("helme")),
    rule(&["char_armor_torso"], "panzerung", Some("brust")),
    rule(&["char_armor_arms"], "panzerung", Some("arme")),
    rule(&["char_armor_legs"], "panzerung", Some("beine")),
    rule(&["char_armor_backpack"], "panzerung", Some("rucksaecke")),
    // A recipe of the armour folder whose product the data does not name.
    rule(&["armour"], "panzerung", None),
    rule(&["char_clothing_hat"], "kleidung", Some("kopfbedeckung")),
    rule(&["char_clothing_torso_0"], "kleidung", Some("hemden")),
    rule(&["char_clothing_torso_1"], "kleidung", Some("jacken")),
    rule(&["char_clothing_hands"], "kleidung", Some("handschuhe")),
    rule(&["char_clothing_legs"], "kleidung", Some("beinbekleidung")),
    rule(&["char_clothing_feet"], "kleidung", Some("schuhwerk")),
    Rule { subtypes: Some(&["pistol"]), ..rule(&["weaponpersonal"], "waffen", Some("seitenwaffen")) },
    Rule {
        subtypes: Some(&["rifle", "smg", "shotgun", "lmg", "sniper"]),
        ..rule(&["weaponpersonal"], "waffen", Some("primaer"))
    },
    // Heavy machine guns and crossbows: the folder names no subkind.
    Rule { subtypes: Some(&[""]), ..rule(&["weaponpersonal"], "waffen", Some("spezial")) },
    Rule { recipe: Some(&["ammo"]), ..rule(&["weaponattachment"], "munition", None) },
    rule(&["ammo"], "munition", None),
    rule(&["cooler"], "vehikel", Some("kuehler")),
    rule(&["powerplant"], "vehikel", Some("kraftwerke")),
    rule(&["quantumdrive"], "vehikel", Some("quantum_drives")),
    rule(&["shield"], "vehikel", Some("schilde")),
    rule(&["radar"], "vehikel", Some("radar")),
    rule(&["weapongun"], "vehikel", Some("waffen")),
    rule(&["weaponmining", "miningmodifier", "mininglaser"], "vehikel", Some("bergbau")),
    rule(&["cargo"], "behaelter", Some("waren_cargo")),
    rule(&["container"], "behaelter", Some("behaelter")),
    // Scraper modules, tractor beams and fuel nozzles stand under miscellaneous in the terminal.
    rule(&["misc", "salvagemodifier", "salvagehead", "tractorbeam", "salvage"], "sonstiges", None),
    rule(&["dockingcollar", "fuelnozzle", "refuelling"], "sonstiges", None),
];

/// The kind the rules compare: the item type of the product, or the kind of
/// the folder where the product has none.
fn kind_of(item_type: &str, recipe_kind: &str) -> String {
    if item_type.is_empty() { recipe_kind.to_lowercase() } else { item_type.to_lowercase() }
}

fn fits(rule: &Rule, kind: &str, recipe_kind: &str, subtype: &str) -> bool {
    rule.kinds.contains(&kind)
        && rule.recipe.is_none_or(|r| r.contains(&recipe_kind))
        && rule.subtypes.is_none_or(|s| s.contains(&subtype))
}

fn place_of(rule: &Rule) -> Place {
    let category = CATEGORIES.iter().position(|c| c.key == rule.category).unwrap_or(MISC);
    Place { category, sub: rule.sub }
}

/// Every place a rule gives the item; one where the rules are right, none
/// for an item no rule knows.
pub fn places(item_type: &str, recipe_kind: &str, subtype: &str) -> Vec<Place> {
    let kind = kind_of(item_type, recipe_kind);
    let (recipe_kind, subtype) = (recipe_kind.to_lowercase(), subtype.to_lowercase());
    RULES.iter().filter(|r| fits(r, &kind, &recipe_kind, &subtype)).map(place_of).collect()
}

/// The place of an item: the first rule that fits, `None` when none does.
pub fn place(item_type: &str, recipe_kind: &str, subtype: &str) -> Option<Place> {
    places(item_type, recipe_kind, subtype).into_iter().next()
}

/// [`place`], with miscellaneous for an item no rule knows: on the page every
/// item stands somewhere.
pub fn place_or_misc(item_type: &str, recipe_kind: &str, subtype: &str) -> Place {
    place(item_type, recipe_kind, subtype).unwrap_or(Place { category: MISC, sub: None })
}

/// The index of a category by its key.
pub fn category_index(key: &str) -> Option<usize> {
    CATEGORIES.iter().position(|c| c.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::{Lang, text_in};

    /// The facts (item type, folder kind, subkind) of every kind of recipe of the
    /// 4.12 build with a name each, as the recipe data of that build holds them.
    const BUILD_4_12: &[(&str, &str, &str, &str)] = &[
        ("Cargo", "", "", "TH-01 Propulsor"),
        ("Container", "", "", "GOLEM MC-4 Ore Pod"),
        ("MiningModifier", "", "", "Clearcut Module"),
        ("Misc", "", "", "Probe"),
        ("WeaponAttachment", "ammo", "ballistic", "a ballistic magazine"),
        ("WeaponAttachment", "ammo", "electron", "an electron magazine"),
        ("WeaponAttachment", "ammo", "laser", "a laser magazine"),
        ("WeaponAttachment", "ammo", "plasma", "a plasma magazine"),
        ("WeaponAttachment", "ammo", "shotgun", "BR-2 Shotgun Magazine"),
        ("Char_Armor_Arms", "armour", "combat", "combat arms"),
        ("Char_Armor_Backpack", "armour", "combat", "combat backpack"),
        ("Char_Armor_Helmet", "armour", "combat", "combat helmet"),
        ("Char_Armor_Legs", "armour", "combat", "combat legs"),
        ("Char_Armor_Torso", "armour", "combat", "combat torso"),
        ("Char_Armor_Undersuit", "armour", "combat", "combat undersuit"),
        ("Char_Armor_Helmet", "armour", "flightsuit", "flight helmet"),
        ("Char_Armor_Undersuit", "armour", "flightsuit", "flight suit"),
        ("Char_Armor_Helmet", "armour", "undersuit", "undersuit helmet"),
        ("Char_Clothing_Feet", "armour", "undersuit", "Spettro Shoes"),
        ("Char_Clothing_Legs", "armour", "undersuit", "Bellator Trousers"),
        ("Char_Clothing_Torso_0", "armour", "undersuit", "Bellator Shirt and Waistcoat"),
        ("Char_Clothing_Torso_1", "armour", "undersuit", "Bellator Jacket"),
        ("Cooler", "cooler", "", "a cooler"),
        // The Pioneer cooler: a recipe whose product the data does not hold.
        ("", "cooler", "", "Pioneer cooler"),
        ("WeaponMining", "mininglaser", "", "a mining laser"),
        ("PowerPlant", "powerplant", "", "a power plant"),
        ("QuantumDrive", "quantumdrive", "size1", "a quantum drive"),
        ("Radar", "radar", "", "a radar"),
        ("DockingCollar", "refuelling", "nozzle", "Norfield"),
        ("SalvageModifier", "salvage", "", "Hart Scraper Module"),
        ("Shield", "shield", "", "a shield"),
        ("SalvageHead", "tractorbeam", "", "SureGrip S3 Tractor Beam"),
        ("TractorBeam", "tractorbeam", "", "SureGrip S1 Tractor Beam"),
        ("WeaponPersonal", "weapons", "", "Vendetta HMG"),
        ("WeaponGun", "weapons", "ballistic", "a ballistic cannon"),
        ("WeaponGun", "weapons", "distortion", "a distortion repeater"),
        ("WeaponGun", "weapons", "laser", "a laser cannon"),
        ("WeaponPersonal", "weapons", "lmg", "F55 LMG"),
        ("WeaponPersonal", "weapons", "pistol", "S-38 Pistol"),
        ("WeaponPersonal", "weapons", "rifle", "P4-AR Rifle"),
        ("WeaponPersonal", "weapons", "shotgun", "R97 Shotgun"),
        ("WeaponPersonal", "weapons", "smg", "Custodian SMG"),
        ("WeaponPersonal", "weapons", "sniper", "A03 Sniper Rifle"),
    ];

    /// The items that do not land in exactly one place, named.
    fn misplaced(facts: &[(&str, &str, &str, &str)]) -> Vec<String> {
        facts
            .iter()
            .filter_map(|(kind, recipe, sub, name)| {
                let found = places(kind, recipe, sub);
                (found.len() != 1).then(|| format!("{name} ({kind}/{recipe}/{sub}): {} places {found:?}", found.len()))
            })
            .collect()
    }

    #[test]
    fn every_kind_of_the_build_lands_in_exactly_one_place() {
        let wrong = misplaced(BUILD_4_12);
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// Counter-probe: an item no rule knows and one two rules know are both named.
    #[test]
    fn an_item_without_a_rule_or_with_two_is_named() {
        let unknown = misplaced(&[("Char_Clothing_Wings", "armour", "", "Wings")]);
        assert_eq!(unknown.len(), 1, "{unknown:?}");
        assert!(unknown[0].starts_with("Wings"), "{unknown:?}");
        assert_eq!(place_or_misc("Char_Clothing_Wings", "armour", ""), Place { category: MISC, sub: None });
        // `armour` is the kind of the folder; it only counts where the item type is missing.
        assert_eq!(place("", "armour", "combat"), Some(Place { category: 0, sub: None }));
        assert!(places("WeaponPersonal", "weapons", "knife").is_empty(), "an unknown subkind has no place");
    }

    #[test]
    fn the_places_are_the_ones_of_the_terminal() {
        // Without texts of the game the own German names stand, as the terminal writes them.
        let names = Names::default();
        let at =
            |kind: &str, recipe: &str, sub: &str| names.path_in(Lang::De, place(kind, recipe, sub).expect("a place"));
        assert_eq!(at("WeaponPersonal", "weapons", "rifle"), "Waffen  ·  Primär");
        assert_eq!(at("WeaponPersonal", "weapons", "pistol"), "Waffen  ·  Seitenwaffen");
        assert_eq!(at("Cargo", "", ""), "Behälter  ·  Waren-Cargo");
        assert_eq!(at("Container", "", ""), "Behälter  ·  Behälter");
        assert_eq!(at("SalvageModifier", "salvage", ""), "Sonstiges");
        assert_eq!(at("DockingCollar", "refuelling", "nozzle"), "Sonstiges");
        assert_eq!(at("WeaponAttachment", "ammo", "laser"), "Munition");
        assert_eq!(at("WeaponGun", "weapons", "laser"), "Vehikel  ·  Waffen");
        assert_eq!(at("Char_Armor_Torso", "armour", "combat"), "Panzerung  ·  Brust");
        assert_eq!(at("", "cooler", ""), "Vehikel  ·  Kühler");
        assert_eq!(at("Char_Clothing_Torso_1", "armour", "undersuit"), "Kleidung  ·  Jacken");
    }

    fn ini(lines: &[(&str, &str)]) -> HashMap<String, String> {
        lines.iter().map(|(k, v)| (format!("{GAME_KEY}{k}"), (*v).to_string())).collect()
    }

    #[test]
    fn the_names_of_the_game_win_where_it_has_good_ones() {
        // German as the game ships it: some texts empty, two categories called like a subcategory.
        let german = ini(&[
            ("armor", "Panzerung"),
            ("undersuits", "Unterzieher"),
            ("pistols", "Handfeuerwaffen"),
            ("vehicle_paint", "Lackierungen"),
            ("vehicle_parts", "Lackierungen"),
            ("containers", "Lackierungen"),
            ("vehicle_radar", ""),
            ("all_categories", "Alle Kategorien"),
        ]);
        let mut other = german.clone();
        other.insert("ui_something_else".into(), "Anderes".into());
        let names = Names::from_ini(&other, &HashMap::new());
        assert_eq!(names.de.len(), german.len() - 1, "only the filter texts with a text are kept: {:?}", names.de);
        assert_eq!(names.sub_in(Lang::De, "undersuits"), "Unterzieher", "the text of the game");
        assert_eq!(names.sub_in(Lang::De, "seitenwaffen"), "Handfeuerwaffen");
        assert_eq!(names.category_in(Lang::De, 0), "Panzerung");
        // Called like a subcategory: the own text.
        assert_eq!(names.category_in(Lang::De, category_index("behaelter").unwrap()), "Behälter");
        assert_eq!(names.category_in(Lang::De, category_index("vehikel").unwrap()), "Vehikel");
        // Empty: the own text.
        assert_eq!(names.sub_in(Lang::De, "radar"), "Radar");
        // English has no texts of the game here: the own ones.
        assert_eq!(names.sub_in(Lang::En, "seitenwaffen"), "Sidearms");
        // Counter-probe: the own text is not simply always taken.
        assert_ne!(Names::default().sub_in(Lang::De, "undersuits"), "Unterzieher");
    }

    #[test]
    fn every_rule_names_a_subcategory_of_its_category() {
        let mut wrong = Vec::new();
        for rule in &RULES {
            match CATEGORIES.iter().find(|c| c.key == rule.category) {
                None => wrong.push(format!("{}: no such category", rule.category)),
                Some(category) => {
                    if let Some(sub) = rule.sub
                        && !category.subs.iter().any(|s| s.key == sub)
                    {
                        wrong.push(format!("{}/{sub}: not a subcategory of it", rule.category));
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        assert_eq!(CATEGORIES[MISC].key, "sonstiges");
        // A subcategory key names one subcategory: the name of the game is found by it.
        let mut keys: Vec<&str> = CATEGORIES.iter().flat_map(|c| c.subs).map(|s| s.key).collect();
        let all = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), all, "a subcategory key stands twice");
    }

    #[test]
    fn every_category_and_subcategory_has_a_name_in_both_languages() {
        let mut missing = Vec::new();
        for lang in [Lang::De, Lang::En] {
            for key in ["s_fb_alle_kat".to_string()]
                .into_iter()
                .chain(CATEGORIES.iter().map(|c| format!("s_fb_kat_{}", c.key)))
                .chain(CATEGORIES.iter().flat_map(|c| c.subs).map(|s| format!("s_fb_un_{}", s.key)))
            {
                if text_in(lang, &key).is_none() {
                    missing.push(format!("{lang:?} {key}"));
                }
            }
        }
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    }
}
