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

//! What the blueprint list asks of the catalog: kinds and their groups,
//! the search, the filters, the reputation cap and what the contracts say,
//! the places as text. Like `catalog.py` and `collection_window.py` of the
//! Python version.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::catalog_cache::{Blueprint, CatalogCache, Place};
use crate::language::{self, Arg};

/// Kinds that mean the same thing and form one group.
///
/// ⚠ Magazines come as `WeaponAttachment` and, for two starter magazines,
/// as `ammo`; hand weapons as `WeaponPersonal` and, for two of them, as
/// `weapons`. Unmerged, a filter would show only a part of them.
const KIND_MERGE: [(&str, &str); 2] = [("ammo", "WeaponAttachment"), ("weapons", "WeaponPersonal")];

/// The kind a blueprint is sorted and filtered by.
pub fn kind_id(raw: &str) -> &str {
    KIND_MERGE.iter().find(|(from, _)| *from == raw).map_or(raw, |(_, to)| to)
}

/// The kind as the window names it (`Char_Armor_Helmet` → „Helm“).
pub fn kind_readable(raw: &str) -> String {
    language::kind_label(kind_id(raw))
}

/// The areas of the list, in this order: the ship, what is held in the
/// hand, what is worn, everything else.
pub const TOP_GROUPS: [&str; 4] = ["schiff", "fps", "ruestung", "sonstiges"];

const KIND_GROUP: [(&str, &str); 26] = [
    ("Cooler", "schiff"),
    ("PowerPlant", "schiff"),
    ("QuantumDrive", "schiff"),
    ("Shield", "schiff"),
    ("Radar", "schiff"),
    ("WeaponGun", "schiff"),
    ("WeaponMining", "schiff"),
    ("SalvageModifier", "schiff"),
    ("SalvageHead", "schiff"),
    ("TractorBeam", "schiff"),
    ("DockingCollar", "schiff"),
    ("FuelNozzle", "schiff"),
    ("Cargo", "schiff"),
    ("WeaponPersonal", "fps"),
    ("WeaponAttachment", "fps"),
    ("weapons", "fps"),
    ("ammo", "fps"),
    ("Char_Armor_Helmet", "ruestung"),
    ("Char_Armor_Torso", "ruestung"),
    ("Char_Armor_Legs", "ruestung"),
    ("Char_Armor_Arms", "ruestung"),
    ("Char_Armor_Backpack", "ruestung"),
    ("Char_Armor_Undersuit", "ruestung"),
    ("Char_Clothing_Torso_0", "ruestung"),
    ("Char_Clothing_Torso_1", "ruestung"),
    ("armour", "ruestung"),
];

/// The area of a kind; anything unknown is `sonstiges`.
pub fn top_group(raw: &str) -> &'static str {
    match KIND_GROUP.iter().find(|(kind, _)| *kind == raw) {
        Some((_, group)) => group,
        // Clothing for legs and feet is worn too; 4.12 shows them under armour.
        None if raw.starts_with("Char_Clothing_") => "ruestung",
        None => "sonstiges",
    }
}

/// More words a kind is found by: some kinds carry the game's English name.
const KIND_KEYWORDS: [(&str, &[&str]); 9] = [
    ("Cooler", &["kühler", "kuehler", "kuhler"]),
    ("PowerPlant", &["generator", "kraftwerk", "energie"]),
    ("QuantumDrive", &["sprungantrieb", "quantenantrieb", "qd"]),
    ("Radar", &["scanner", "ortung"]),
    ("Shield", &["schild", "schilde"]),
    ("WeaponGun", &["kanone", "geschütz", "geschuetz"]),
    ("WeaponPersonal", &["gewehr", "pistole", "fps"]),
    ("TractorBeam", &["traktor", "schlepper"]),
    ("Cargo", &["fracht", "ladung"]),
];

pub fn keywords(raw: &str) -> &'static [&'static str] {
    KIND_KEYWORDS.iter().find(|(kind, _)| *kind == raw).map_or(&[], |(_, words)| words)
}

// ------------------------------------------------------------ Categories

/// The groups of the kind filter, in their order.
pub const TOP_CATEGORIES: [&str; 8] =
    ["schiffswaffe", "schiffsmodul", "schiffswerkzeug", "fpswaffe", "ausruestung", "ruestung", "kleidung", "sonstiges"];

const FROM_KIND: [(&str, (&str, &str)); 27] = [
    ("char_armor_helmet", ("ruestung", "helm")),
    ("char_armor_torso", ("ruestung", "torso")),
    ("char_armor_arms", ("ruestung", "arme")),
    ("char_armor_legs", ("ruestung", "beine")),
    ("char_armor_undersuit", ("ruestung", "unteranzug")),
    ("char_armor_backpack", ("ruestung", "rucksack")),
    ("backpack", ("ruestung", "rucksack")),
    ("undersuit", ("ruestung", "unteranzug")),
    ("char_clothing_torso", ("kleidung", "oberkoerper")),
    ("char_clothing_legs", ("kleidung", "beine")),
    ("char_clothing_feet", ("kleidung", "schuhe")),
    ("char_clothing_jacket", ("kleidung", "jacke")),
    ("cooler", ("schiffsmodul", "cooler")),
    ("powerplant", ("schiffsmodul", "powerplant")),
    ("quantumdrive", ("schiffsmodul", "quantumdrive")),
    ("shield", ("schiffsmodul", "schild")),
    ("radar", ("schiffsmodul", "radar")),
    ("weaponattachment", ("ausruestung", "aufsatz")),
    ("weaponmagazine", ("ausruestung", "magazin")),
    ("magazine", ("ausruestung", "magazin")),
    ("dockingcollar", ("schiffswerkzeug", "andockkragen")),
    ("fuelnozzle", ("schiffswerkzeug", "andockkragen")),
    ("weaponmining", ("schiffswerkzeug", "mining_laser")),
    ("container", ("ausruestung", "behaelter")),
    ("cargomodule", ("schiffswerkzeug", "frachtmodul")),
    ("salvagemodifier", ("schiffswerkzeug", "salvage_modifier")),
    ("salvagehead", ("schiffswerkzeug", "salvage_head")),
];

/// Category and subcategory of a blueprint from its kind and subkind: the
/// group key (or `art:<kind>` for a kind that stands alone) and the
/// subcategory key (empty if unknown).
///
/// ⚠ Without the recipe tag and subtype, ship weapons and hand weapons have
/// no subcategory; [`classify_blueprint`] takes them where the catalog
/// carries them.
pub fn classify(kind: &str, sub: &str) -> (String, &'static str) {
    let low = kind.to_lowercase();
    if sub.eq_ignore_ascii_case("magazine") && low == "weaponattachment" {
        return ("ausruestung".into(), "magazin");
    }
    if let Some((_, (top, subcategory))) = FROM_KIND.iter().find(|(k, _)| *k == low) {
        return ((*top).into(), subcategory);
    }
    if low == "ammo" {
        return ("ausruestung".into(), "munition");
    }
    let top = if low.starts_with("weapongun") {
        "schiffswaffe"
    } else if low.starts_with("weapon") {
        "fpswaffe"
    } else if low.starts_with("char_armor") || low == "armour" {
        "ruestung"
    } else if low.starts_with("char_clothing") {
        "kleidung"
    } else if !kind.is_empty() {
        return (format!("art:{kind}"), "");
    } else {
        "sonstiges"
    };
    (top.into(), "")
}

/// The kinds a recipe tag names (`BP_CRAFT_APAR_BallisticGatling_S4`); the
/// compound ones come first so that `BallisticScatterGun` is not taken for
/// `ScatterGun`.
const TAG_KINDS: [(&str, (&str, &str)); 18] = [
    ("ballisticscattergun", ("schiffswaffe", "scatter_gun")),
    ("laserscattergun", ("schiffswaffe", "scatter_gun")),
    ("ballisticcannon", ("schiffswaffe", "ballistic_cannon")),
    ("ballisticgatling", ("schiffswaffe", "ballistic_gatling")),
    ("ballisticrepeater", ("schiffswaffe", "ballistic_repeater")),
    ("lasercannon", ("schiffswaffe", "laser_cannon")),
    ("laserrepeater", ("schiffswaffe", "laser_repeater")),
    ("distortioncannon", ("schiffswaffe", "dist_cannon")),
    ("distortionrepeater", ("schiffswaffe", "dist_repeater")),
    ("neutroncannon", ("schiffswaffe", "neutron_cannon")),
    ("neutronrepeater", ("schiffswaffe", "neutron_repeater")),
    ("tachyoncannon", ("schiffswaffe", "tachyon_cannon")),
    ("scattergun", ("schiffswaffe", "scatter_gun")),
    ("massdriver", ("schiffswaffe", "mass_driver")),
    ("mininglaser", ("schiffswerkzeug", "mining_laser")),
    ("salvagemodifier", ("schiffswerkzeug", "salvage_modifier")),
    ("salvagehead", ("schiffswerkzeug", "salvage_head")),
    ("tractorbeam", ("schiffswerkzeug", "tractor_beam")),
];

/// Whether `tag` holds `_<word>` followed by `_` or its end, found at the
/// first underscore it fits at (`re.search(r'_(word)(?:_|$)', tag, re.I)`).
fn tag_word_at<'a>(tag: &str, words: impl Iterator<Item = &'a str> + Clone) -> Option<&'a str> {
    let lower = tag.to_lowercase();
    for (i, _) in lower.match_indices('_') {
        let rest = &lower[i + 1..];
        for word in words.clone() {
            if rest.starts_with(word) && rest[word.len()..].chars().next().is_none_or(|c| c == '_') {
                return Some(word);
            }
        }
    }
    None
}

/// Category and subcategory of a recipe, from the four things 4.12 reads:
/// the kind of the catalog (`Char_Armor_Helmet`), the recipe tag, the subtype
/// of the recipe (`pistol`) and its kind (`ammo`) (`categories.classify`).
///
/// The tag is the most exact, the kind the most reliable, the subtype the
/// coarsest: in that order.
pub fn classify_recipe(kind: &str, tag: &str, subtype: &str, recipe_kind: &str) -> (String, &'static str) {
    if tag_word_at(tag, std::iter::once("mag")).is_some() {
        return ("ausruestung".into(), "magazin");
    }
    if let Some(word) = tag_word_at(tag, TAG_KINDS.iter().map(|(word, _)| *word))
        && let Some((_, (top, sub))) = TAG_KINDS.iter().find(|(w, _)| *w == word)
    {
        return ((*top).into(), sub);
    }
    let low = kind.to_lowercase();
    if let Some((_, (top, sub))) = FROM_KIND.iter().find(|(k, _)| *k == low) {
        return ((*top).into(), sub);
    }
    let fps = match subtype.to_lowercase().as_str() {
        "pistol" => Some("pistole"),
        "rifle" => Some("gewehr"),
        "sniper" => Some("sniper"),
        "smg" => Some("smg"),
        "shotgun" => Some("schrotflinte"),
        "lmg" => Some("lmg"),
        _ => None,
    };
    if let Some(sub) = fps {
        return ("fpswaffe".into(), sub);
    }
    if recipe_kind.eq_ignore_ascii_case("ammo") {
        return ("ausruestung".into(), "munition");
    }
    if low.starts_with("weapongun") {
        return ("schiffswaffe".into(), "");
    }
    if low.starts_with("weapon") {
        return ("fpswaffe".into(), "");
    }
    if low.starts_with("char_armor") {
        return ("ruestung".into(), "");
    }
    if low.starts_with("char_clothing") {
        return ("kleidung".into(), "");
    }
    if !kind.is_empty() {
        return (format!("art:{kind}"), "");
    }
    ("sonstiges".into(), "")
}

/// Category and subcategory of a blueprint of the catalog: from its recipe
/// ([`classify_recipe`]) where the catalog carries one, else from its kind
/// and subkind ([`classify`]).
pub fn classify_blueprint(blueprint: &Blueprint) -> (String, &'static str) {
    if blueprint.recipe_tag.is_empty() && blueprint.recipe_subtype.is_empty() && blueprint.recipe_kind.is_empty() {
        return classify(&blueprint.kind, &blueprint.sub);
    }
    classify_recipe(&blueprint.kind, &blueprint.recipe_tag, &blueprint.recipe_subtype, &blueprint.recipe_kind)
}

/// How a category is called in the window; a kind standing alone by its kind name.
pub fn category_name(key: &str) -> String {
    match key.strip_prefix("art:") {
        Some(kind) => kind_readable(kind),
        None => language::t(&format!("kat_ober_{key}")),
    }
}

/// How a subcategory is called in the window.
pub fn subcategory_name(key: &str) -> String {
    language::t(&format!("kat_unter_{key}"))
}

// ------------------------------------------------------------ Groups

/// One group of the list: area, readable kind, the blueprints (indices into
/// the catalog), sorted by size and then by name.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub area: &'static str,
    pub title: String,
    pub rows: Vec<usize>,
}

/// The blueprints `shown` in groups, in display order: areas in the order of
/// `TOP_GROUPS`, kinds within an area alphabetically, rows by size (no size
/// last), then by name.
pub fn groups(catalog: &CatalogCache, shown: &[usize]) -> Vec<Group> {
    let mut by_group: BTreeMap<(usize, String), (String, Vec<usize>)> = BTreeMap::new();
    for &i in shown {
        let blueprint = &catalog.blueprints[i];
        let raw = kind_id(&blueprint.kind);
        let area = top_group(raw);
        let title = kind_readable(raw);
        let order = TOP_GROUPS.iter().position(|g| *g == area).unwrap_or(TOP_GROUPS.len());
        by_group.entry((order, title.to_lowercase())).or_insert_with(|| (title, Vec::new())).1.push(i);
    }
    by_group
        .into_iter()
        .map(|((order, _), (title, mut rows))| {
            rows.sort_by_cached_key(|&i| {
                let blueprint = &catalog.blueprints[i];
                crate::size_order::key(blueprint.size, &blueprint.name)
            });
            Group { area: TOP_GROUPS.get(order).copied().unwrap_or("sonstiges"), title, rows }
        })
        .collect()
}

/// Share of blueprints of a kind that must differ from the most common value
/// for a field (size, grade) to say anything about that kind.
pub const SPREAD_SHARE: f64 = 0.10;

/// The kinds for which a field (size or grade) really varies: at least
/// `SPREAD_SHARE` of their blueprints differ from the most common value. For
/// armour every piece has size 1 and grade A, so the field says nothing there.
pub fn kinds_with_spread(catalog: &CatalogCache, field: fn(&Blueprint) -> Option<i64>) -> HashSet<String> {
    let mut per_kind: HashMap<&str, HashMap<i64, usize>> = HashMap::new();
    for blueprint in &catalog.blueprints {
        if let Some(value) = field(blueprint) {
            *per_kind.entry(kind_id(&blueprint.kind)).or_default().entry(value).or_default() += 1;
        }
    }
    per_kind
        .into_iter()
        .filter(|(_, counts)| {
            let total: usize = counts.values().sum();
            let most = counts.values().copied().max().unwrap_or(0);
            total > 0 && (total - most) as f64 / total as f64 >= SPREAD_SHARE
        })
        .map(|(kind, _)| kind.to_string())
        .collect()
}

// ------------------------------------------------------------ Search

/// Does the search text (lower case, trimmed) hit this blueprint? Name,
/// class, manufacturer, grade („grade a“, „grad a“, „a“), size („size 2“,
/// „größe 2“, „s2“), and every contract: title, faction, type.
pub fn matches(blueprint: &Blueprint, text: &str) -> bool {
    let has = |value: &str| value.to_lowercase().contains(text);
    if has(&blueprint.name)
        || blueprint.class.as_deref().is_some_and(has)
        || blueprint.maker.as_deref().is_some_and(has)
    {
        return true;
    }
    if let Some(letter) = blueprint.grade.and_then(crate::catalog_cache::grade_letter) {
        let letter = letter.to_ascii_lowercase();
        if [format!("grade {letter}"), format!("grad {letter}"), letter.to_string()].iter().any(|t| t == text) {
            return true;
        }
    }
    if let Some(size) = blueprint.size
        && [format!("size {size}"), format!("größe {size}"), format!("s{size}")].iter().any(|t| t == text)
    {
        return true;
    }
    blueprint.ways.iter().any(|way| has(&way.contract) || has(&way.faction) || has(&way.kind))
}

/// Does the search text hit the whole kind (its name or one of its words)?
pub fn hits_kind(raw: &str, text: &str) -> bool {
    let raw = kind_id(raw);
    kind_readable(raw).to_lowercase().contains(text) || keywords(raw).iter().any(|word| word.contains(text))
}

/// The contracts whose title holds the search text, with how many blueprints
/// each gives, most first, then by title; at most `limit`.
pub fn contracts_with(catalog: &CatalogCache, text: &str, limit: usize) -> Vec<(String, usize)> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for blueprint in &catalog.blueprints {
        let mut seen = HashSet::new();
        for way in &blueprint.ways {
            let title = way.contract.trim();
            if !title.is_empty() && seen.insert(title) && title.to_lowercase().contains(text) {
                *counts.entry(title).or_default() += 1;
            }
        }
    }
    let mut list: Vec<(String, usize)> = counts.into_iter().map(|(t, n)| (t.to_string(), n)).collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list.truncate(limit);
    list
}

// ------------------------------------------------------------ Contracts

/// The blueprints that can be lost by rising too high: every contract that
/// gives them has a reputation cap. Key → (cap, rank) of the most generous one.
pub fn reward_caps(catalog: &CatalogCache) -> HashMap<String, (i64, Option<String>)> {
    let mut ways: HashMap<&str, Vec<usize>> = HashMap::new();
    for (m, mission) in catalog.missions.iter().enumerate() {
        let keys: HashSet<&str> = mission.blueprints.iter().map(String::as_str).collect();
        for key in keys {
            ways.entry(key).or_default().push(m);
        }
    }
    let mut caps = HashMap::new();
    for (key, missions) in ways {
        let missions: Vec<_> = missions.iter().map(|&m| &catalog.missions[m]).collect();
        if missions.iter().any(|m| m.reputation_max.is_none_or(|r| r == 0)) {
            continue;
        }
        if let Some(highest) = missions.iter().max_by_key(|m| m.reputation_max.unwrap_or(0)) {
            caps.insert(key.to_string(), (highest.reputation_max.unwrap_or(0), highest.rank_max.clone()));
        }
    }
    caps
}

/// What the contracts of a blueprint say: shareable (only if all are;
/// `None` without contracts) and the shortest lockout in minutes.
pub fn contract_traits(catalog: &CatalogCache, key: &str) -> (Option<bool>, Option<i64>) {
    let missions: Vec<_> = catalog.missions.iter().filter(|m| m.blueprints.iter().any(|k| k == key)).collect();
    if missions.is_empty() {
        return (None, None);
    }
    let shareable = missions.iter().all(|m| m.shareable == Some(true));
    let lockout = missions.iter().filter_map(|m| m.cooldown).filter(|c| *c > 0).min();
    (Some(shareable), lockout)
}

/// A contract worth doing: title, faction, how many missing blueprints its
/// reward pool holds, the highest reward, the rank and where it is taken.
#[derive(Clone, Debug, PartialEq)]
pub struct Worthwhile {
    pub title: String,
    pub faction: String,
    pub missing: usize,
    pub reward: i64,
    pub rank: Option<String>,
    pub place: Option<Place>,
}

/// Which contracts give the most blueprints one does not have yet, most
/// first, then the higher reward, then by title. Counted over the ways of
/// the blueprints, which carry the readable title.
pub fn worthwhile_contracts(catalog: &CatalogCache, have: &HashSet<String>) -> Vec<Worthwhile> {
    let mut contracts: HashMap<(String, String), Worthwhile> = HashMap::new();
    for blueprint in catalog.blueprints.iter().filter(|b| !have.contains(&b.key)) {
        for way in blueprint.ways.iter().filter(|w| !w.contract.is_empty()) {
            let entry = contracts.entry((way.contract.clone(), way.faction.clone())).or_insert_with(|| Worthwhile {
                title: way.contract.clone(),
                faction: way.faction.clone(),
                missing: 0,
                reward: 0,
                rank: way.rank.clone(),
                place: way.place.clone(),
            });
            entry.missing += 1;
            entry.reward = entry.reward.max(way.reward.unwrap_or(0));
        }
    }
    let mut list: Vec<Worthwhile> = contracts.into_values().collect();
    list.sort_by(|a, b| b.missing.cmp(&a.missing).then(b.reward.cmp(&a.reward)).then_with(|| a.title.cmp(&b.title)));
    list
}

/// Minutes as text: „15 Min“, „2 Std 30 Min“, „7 Tagen“.
pub fn duration_text(minutes: i64) -> String {
    if minutes < 60 {
        return language::tf("zeit_min", &[("0", Arg::from(minutes))]);
    }
    if minutes < 24 * 60 {
        let (hours, rest) = (minutes / 60, minutes % 60);
        return if rest == 0 {
            language::tf("zeit_std", &[("0", Arg::from(hours))])
        } else {
            language::tf("zeit_std_min", &[("0", Arg::from(hours)), ("1", Arg::from(rest))])
        };
    }
    let days = (minutes as f64 / (24.0 * 60.0)).round_ties_even() as i64;
    language::tf(if days == 1 { "zeit_tag" } else { "zeit_tage" }, &[("0", Arg::from(days))])
}

/// Where a contract is taken: „Annehmen in Stanton: Hurston, Crusader und 2 weiteren“.
pub fn place_text(place: &Place) -> String {
    let mut places = place.places.join(", ");
    if place.more > 0 {
        places.push_str(&language::tf("und_weitere", &[("0", Arg::from(place.more))]));
    }
    let take = language::t("annehmen_in");
    if !place.system.is_empty() && !places.is_empty() {
        format!("{take} {}: {places}", place.system)
    } else {
        format!("{take} {}", if place.system.is_empty() { &places } else { &place.system })
    }
}

// ------------------------------------------------------------ Patches

/// „4.10.0-live.12519617“ → „4.10.0“.
pub fn version_short(version: &str) -> &str {
    version.split('-').next().filter(|s| !s.is_empty()).unwrap_or(version)
}

/// Sort key of a game version: the numbers of the version, then the build.
fn version_rank(version: &str) -> Vec<u64> {
    version.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect()
}

/// Every game version blueprints came with: (full, short, count), newest first.
pub fn patches(catalog: &CatalogCache) -> Vec<(String, String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for blueprint in &catalog.blueprints {
        if let Some(since) = blueprint.since.as_deref() {
            *counts.entry(since).or_default() += 1;
        }
    }
    let mut list: Vec<(String, String, usize)> =
        counts.into_iter().map(|(v, n)| (v.to_string(), version_short(v).to_string(), n)).collect();
    list.sort_by_key(|(v, _, _)| std::cmp::Reverse(version_rank(v)));
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_cache::{Mission, Way};

    fn blueprint(name: &str, kind: &str, size: i64, grade: i64) -> Blueprint {
        Blueprint {
            key: crate::paths::name_key(name),
            name: name.into(),
            kind: kind.into(),
            size: Some(size),
            grade: Some(grade),
            ..Blueprint::default()
        }
    }

    #[test]
    fn groups_follow_the_areas() {
        let catalog = CatalogCache {
            blueprints: vec![
                blueprint("Zeta Helm", "Char_Armor_Helmet", 1, 1),
                blueprint("Alpha Cooler", "Cooler", 2, 3),
                blueprint("S-38 Pistol", "weapons", 1, 1),
                blueprint("Beta Cooler", "Cooler", 1, 1),
                blueprint("Box", "Container", 1, 1),
            ],
            ..CatalogCache::default()
        };
        let all: Vec<usize> = (0..5).collect();
        let groups = groups(&catalog, &all);
        let areas: Vec<&str> = groups.iter().map(|g| g.area).collect();
        assert_eq!(areas, ["schiff", "fps", "ruestung", "sonstiges"]);
        assert_eq!(groups[0].rows, [3, 1], "rows by size first: Beta S1 before Alpha S2");
        assert_eq!(kind_id("weapons"), "WeaponPersonal");
    }

    #[test]
    fn rows_follow_the_size_not_the_letters_of_a_roman_number() {
        let names = [("Omnisky IX Cannon", 3), ("Omnisky III Cannon", 1), ("Omnisky XV Cannon", 5), ("Omnisky VI Cannon", 2)];
        let mut blueprints: Vec<Blueprint> = names.iter().map(|(n, s)| blueprint(n, "WeaponGun", *s, 1)).collect();
        blueprints.push(Blueprint { size: None, ..blueprint("Aaron Cannon", "WeaponGun", 1, 1) });
        let catalog = CatalogCache { blueprints, ..CatalogCache::default() };
        let all: Vec<usize> = (0..5).collect();
        let rows = &groups(&catalog, &all)[0].rows;
        let shown: Vec<&str> = rows.iter().map(|&i| catalog.blueprints[i].name.as_str()).collect();
        assert_eq!(
            shown,
            ["Omnisky III Cannon", "Omnisky VI Cannon", "Omnisky IX Cannon", "Omnisky XV Cannon", "Aaron Cannon"],
            "by size, one without a size last"
        );
    }

    #[test]
    fn a_backpack_is_a_subcategory_of_armour() {
        assert_eq!(classify("Char_Armor_Backpack", ""), ("ruestung".to_string(), "rucksack"));
        assert_eq!(classify_recipe("Char_Armor_Backpack", "", "", ""), ("ruestung".to_string(), "rucksack"));
        assert_eq!(top_group("Char_Armor_Backpack"), "ruestung", "the area and the category agree");
    }

    /// With a recipe the tag and the subtype split the weapons; without one kind and subkind decide.
    #[test]
    fn a_blueprint_with_a_recipe_gets_the_subcategory_of_the_recipe() {
        let recipe = |kind: &str, sub: &str, tag: &str, subtype: &str, recipe_kind: &str| Blueprint {
            kind: kind.into(),
            sub: sub.into(),
            recipe_tag: tag.into(),
            recipe_subtype: subtype.into(),
            recipe_kind: recipe_kind.into(),
            ..Blueprint::default()
        };
        let gatling = recipe("WeaponGun", "Gun", "BP_CRAFT_APAR_BallisticGatling_S4", "ballistic", "weapons");
        assert_eq!(classify_blueprint(&gatling), ("schiffswaffe".to_string(), "ballistic_gatling"));
        let pistol = recipe("WeaponPersonal", "Small", "BP_CRAFT_behr_pistol_01", "pistol", "weapons");
        assert_eq!(classify_blueprint(&pistol), ("fpswaffe".to_string(), "pistole"));
        let magazine = recipe("WeaponAttachment", "Magazine", "BP_CRAFT_behr_pistol_mag_01", "", "ammo");
        assert_eq!(classify_blueprint(&magazine), ("ausruestung".to_string(), "magazin"));
        // Without any recipe: as before from kind and subkind.
        let bare = recipe("WeaponAttachment", "Magazine", "", "", "");
        assert_eq!(classify_blueprint(&bare), classify("WeaponAttachment", "Magazine"));
        let gun = recipe("WeaponGun", "Gun", "", "", "");
        assert_eq!(classify_blueprint(&gun), ("schiffswaffe".to_string(), ""));
    }

    #[test]
    fn size_and_grade_count_only_where_they_vary() {
        let mut blueprints: Vec<Blueprint> = (0..10).map(|_| blueprint("Helm", "Char_Armor_Helmet", 1, 1)).collect();
        blueprints.extend((0..10).map(|i| blueprint("Cooler", "Cooler", i % 3 + 1, i % 4 + 1)));
        let catalog = CatalogCache { blueprints, ..CatalogCache::default() };
        let spread = kinds_with_spread(&catalog, |b| b.grade);
        assert!(spread.contains("Cooler") && !spread.contains("Char_Armor_Helmet"), "{spread:?}");
    }

    #[test]
    fn search_finds_name_grade_size_and_contract() {
        let mut cooler = blueprint("ArcticStorm", "Cooler", 1, 3);
        cooler.maker = Some("Lightning Power Ltd.".into());
        cooler.ways = vec![Way { contract: "Protect Fuel Tanks".into(), faction: "Foxwell".into(), ..Way::default() }];
        for hit in ["arctic", "grade c", "c", "size 1", "s1", "lightning", "fuel", "foxwell"] {
            assert!(matches(&cooler, hit), "{hit}");
        }
        for miss in ["grade a", "1", "size 2", "nyx"] {
            assert!(!matches(&cooler, miss), "{miss}");
        }
    }

    #[test]
    fn a_cap_counts_only_if_every_way_has_one() {
        let mission = |keys: &[&str], cap: Option<i64>| Mission {
            key: String::new(),
            blueprints: keys.iter().map(|k| k.to_string()).collect(),
            reputation_max: cap,
            rank_max: cap.map(|c| format!("Rang {c}")),
            shareable: Some(cap.is_some()),
            cooldown: cap.map(|_| 15),
            ..Mission::default()
        };
        let catalog = CatalogCache {
            missions: vec![mission(&["a", "b"], Some(1000)), mission(&["a"], Some(5000)), mission(&["b"], None)],
            ..CatalogCache::default()
        };
        let caps = reward_caps(&catalog);
        assert_eq!(caps.get("a"), Some(&(5000, Some("Rang 5000".into()))));
        assert!(!caps.contains_key("b"), "an open way is enough");
        assert_eq!(contract_traits(&catalog, "a"), (Some(true), Some(15)));
        assert_eq!(contract_traits(&catalog, "b"), (Some(false), Some(15)));
        assert_eq!(contract_traits(&catalog, "c"), (None, None));
    }

    #[test]
    fn worthwhile_counts_only_missing_blueprints() {
        let way = |contract: &str, reward: i64| Way {
            contract: contract.into(),
            faction: "Foxwell".into(),
            reward: Some(reward),
            ..Way::default()
        };
        let mut a = blueprint("A", "Cooler", 1, 1);
        a.ways = vec![way("Escort", 1000), way("Protect", 500)];
        let mut b = blueprint("B", "Cooler", 1, 1);
        b.ways = vec![way("Escort", 3000)];
        let mut c = blueprint("C", "Cooler", 1, 1);
        c.ways = vec![way("Protect", 9000)];
        let catalog = CatalogCache { blueprints: vec![a, b, c], ..CatalogCache::default() };
        let have: HashSet<String> = [crate::paths::name_key("C")].into_iter().collect();
        let list = worthwhile_contracts(&catalog, &have);
        assert_eq!(list[0].title, "Escort");
        assert_eq!((list[0].missing, list[0].reward), (2, 3000));
        assert_eq!((list[1].title.as_str(), list[1].missing, list[1].reward), ("Protect", 1, 500));
    }

    #[test]
    fn patches_newest_first() {
        let mut a = blueprint("A", "Cooler", 1, 1);
        a.since = Some("4.10.0-live.12519617".into());
        let mut b = blueprint("B", "Cooler", 1, 1);
        b.since = Some("4.10.2-live.12881860".into());
        let mut c = b.clone();
        c.since = Some("4.9.0-live.1".into());
        let catalog = CatalogCache { blueprints: vec![a, b, c], ..CatalogCache::default() };
        let shorts: Vec<String> = patches(&catalog).into_iter().map(|(_, s, _)| s).collect();
        assert_eq!(shorts, ["4.10.2", "4.10.0", "4.9.0"]);
    }

    #[test]
    fn places_and_durations_as_text() {
        language::set_language(language::Choice::De);
        let place = Place { system: "Stanton".into(), places: vec!["Hurston".into(), "Crusader".into()], more: 2 };
        assert_eq!(place_text(&place), "Annehmen in Stanton: Hurston, Crusader und 2 weiteren");
        assert_eq!(duration_text(15), "15 Min");
        assert_eq!(duration_text(150), "2 Std 30 Min");
        assert_eq!(duration_text(10080), "7 Tagen");
    }
}
