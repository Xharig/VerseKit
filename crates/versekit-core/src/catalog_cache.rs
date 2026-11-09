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

//! The blueprint catalog as the Python version caches it
//! (`katalog-cache.json`): every blueprint with kind, size, grade, class,
//! manufacturer and the contracts that give it. Read only.

use std::collections::HashMap;

use crate::jsonio::{self, Map, Value};
use crate::paths::{Paths, name_key};

/// The cache file of the 4.12 version, in the subfolder `Intern`.
pub const CACHE_FILE: &str = "katalog-cache.json";

/// The cache file of the catalog this version builds (from the game data and
/// scmdb), in the same format and the same subfolder.
pub const LIST_CACHE_FILE: &str = "katalog-spieldaten.json";

/// The field of [`LIST_CACHE_FILE`] with the text keys the game shows for each crafted item: record GUID of the
/// item in lower case → `{"name": "item_Name…", "text": "item_Desc…"}`. Several items can share one description.
/// Missing in the 4.12 cache and in a catalog built without the game data.
pub const ITEM_TEXTS: &str = "beschreibungen";

/// The file the pages that only read the catalog take it from: the one this
/// version builds, else (an upgrader who has not started the list yet) the
/// one of 4.12.
pub fn current_file(paths: &Paths) -> String {
    let own = paths.app_file(LIST_CACHE_FILE);
    if std::path::Path::new(&own).is_file() { own } else { paths.app_file(CACHE_FILE) }
}

/// Where a contract is taken: system, places, how many more places.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub system: String,
    pub places: Vec<String>,
    pub more: i64,
}

/// One way to a blueprint: a contract that gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Way {
    /// Contract title (`auftrag`).
    pub contract: String,
    /// Contract type (`typ`), e.g. „Mercenary“.
    pub kind: String,
    /// Faction (`fraktion`).
    pub faction: String,
    /// Reward in aUEC (`uec`).
    pub reward: Option<i64>,
    /// Reputation the contract gives (`ruf`).
    pub reputation_gain: Option<i64>,
    /// Rank needed (`rang`) and the reputation that rank starts at (`rep`).
    pub rank: Option<String>,
    pub rank_reputation: Option<i64>,
    /// Where it is taken (`wo`).
    pub place: Option<Place>,
}

/// One blueprint of the catalog.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blueprint {
    /// The key in the catalog (lower case name).
    pub key: String,
    /// Name (`n`).
    pub name: String,
    /// Kind as the game data names it (`a`), e.g. `Cooler`, `Char_Armor_Helmet`.
    pub kind: String,
    /// Subkind (`sub`), e.g. `Light`, `Gun`; `UNDEFINED` where there is none.
    pub sub: String,
    /// Size (`s`) and grade as a number, 1 = A (`g`).
    pub size: Option<i64>,
    pub grade: Option<i64>,
    /// Class (`c`), e.g. `Military`; only ship components have one.
    pub class: Option<String>,
    /// Manufacturer (`m`).
    pub maker: Option<String>,
    /// The contracts that give it (`q`).
    pub ways: Vec<Way>,
    /// Everybody has it from the start (`start`).
    pub start: bool,
    /// A reward pool outside regular contracts (`topf`).
    pub pool: Option<String>,
    /// The game version it came with (`seit`).
    pub since: Option<String>,
    /// No known way to get it (`ohne_weg`); hidden unless `alle_bauplaene` is set.
    pub no_way: bool,
    /// The recipe as the game data name it: its tag (`rezept_tag`, e.g.
    /// `BP_CRAFT_APAR_BallisticGatling_S4`), the subtype its folder names
    /// (`rezept_unterart`, e.g. `pistol`) and its kind (`rezept_art`, e.g.
    /// `weapons`, `ammo`). Empty where the catalog does not carry them (scmdb's
    /// catalog, the 4.12 cache).
    pub recipe_tag: String,
    pub recipe_subtype: String,
    pub recipe_kind: String,
}

/// A contract as the catalog keeps it for blueprints (`missionen`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mission {
    /// The key of the contract text (`titleLocKey`, else `descriptionLocKey`,
    /// without a leading `@`): the same in every language, what the log and
    /// the game texts name a contract by. Empty for a catalog that did not
    /// keep it; `to_value` then numbers the missions.
    pub key: String,
    /// Keys of the blueprints it gives (`bp`, as name keys).
    pub blueprints: Vec<String>,
    /// Highest reputation at which it is still offered (`rep_max`) and that rank (`rang_max`).
    pub reputation_max: Option<i64>,
    pub rank_max: Option<String>,
    /// Can be shared in a party (`teilbar`).
    pub shareable: Option<bool>,
    /// Minutes until it can be taken again (`cooldown`).
    pub cooldown: Option<i64>,
    /// The other values 4.12 keeps per contract text, in the form of the
    /// file: `sicher`, `chance`, `rep`, `rang`, `uec`, `ruf`, `ab`,
    /// `rep_eindeutig`, `je_system`, `leer`, `stufen`, `titel_key`,
    /// `text_key`. The injection of the blueprint notes reads them. Empty
    /// for a catalog that did not keep them.
    pub extra: Map,
}

/// The keys of a contract in the order 4.12 writes them.
pub const MISSION_KEYS: [&str; 18] = [
    "bp",
    "sicher",
    "chance",
    "rep",
    "rang",
    "rep_max",
    "rang_max",
    "uec",
    "ruf",
    "teilbar",
    "cooldown",
    "ab",
    "rep_eindeutig",
    "je_system",
    "leer",
    "stufen",
    "titel_key",
    "text_key",
];

/// The keys [`Mission`] holds in fields of its own.
const OWN_MISSION_KEYS: [&str; 5] = ["bp", "rep_max", "rang_max", "teilbar", "cooldown"];

/// The whole catalog.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CatalogCache {
    /// Game version of the data (`version`) and when it was fetched (`geholt`).
    pub version: String,
    pub fetched: String,
    /// In the order of the file.
    pub blueprints: Vec<Blueprint>,
    pub missions: Vec<Mission>,
    /// Whom the contracts credit reputation, where scmdb's contract file was
    /// read (`auftragsruf.json`); never written into the catalog file.
    pub reputation: Option<Box<crate::injection::reputation::Table>>,
}

fn int(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) if x.is_finite() => Some(*x as i64),
        _ => None,
    }
}

fn text(value: Option<&Value>) -> Option<String> {
    value?.as_str().map(str::to_string).filter(|s| !s.is_empty())
}

fn place(value: Option<&Value>) -> Option<Place> {
    let value = value?;
    value.as_object()?;
    let places = match value.get("orte") {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    };
    Some(Place {
        system: text(value.get("system")).unwrap_or_default(),
        places,
        more: int(value.get("mehr")).unwrap_or(0),
    })
}

fn way(value: &Value) -> Way {
    Way {
        contract: text(value.get("auftrag")).unwrap_or_default(),
        kind: text(value.get("typ")).unwrap_or_default(),
        faction: text(value.get("fraktion")).unwrap_or_default(),
        reward: int(value.get("uec")),
        reputation_gain: int(value.get("ruf")),
        rank: text(value.get("rang")),
        rank_reputation: int(value.get("rep")),
        place: place(value.get("wo")),
    }
}

/// One contract from its entry in `missionen` (`key` is the key of the
/// entry); anything missing stays empty, the values [`Mission`] has no field
/// for go to `extra`.
pub fn mission_from_value(key: &str, mission: &Value) -> Mission {
    Mission {
        key: key.to_string(),
        blueprints: match mission.get("bp") {
            Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).map(name_key).collect(),
            _ => Vec::new(),
        },
        reputation_max: int(mission.get("rep_max")),
        rank_max: text(mission.get("rang_max")),
        shareable: match mission.get("teilbar") {
            Some(Value::Bool(b)) => Some(*b),
            _ => None,
        },
        cooldown: int(mission.get("cooldown")),
        extra: mission
            .as_object()
            .map(|map| map.iter().filter(|(k, _)| !OWN_MISSION_KEYS.contains(&k.as_str())).cloned().collect())
            .unwrap_or_default(),
    }
}

/// The catalog from the parsed cache file; anything missing stays empty.
///
/// ⚠ Keys are rebuilt from the names (`paths::name_key`), because a cache
/// written by an older version can hold keys in an older form.
pub fn parse(root: &Value) -> CatalogCache {
    let blueprints = match root.get("bauplaene") {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(key, entry)| Blueprint {
                key: text(entry.get("n")).map_or_else(|| key.clone(), |n| name_key(&n)),
                name: text(entry.get("n")).unwrap_or_else(|| key.clone()),
                kind: text(entry.get("a")).unwrap_or_default(),
                sub: text(entry.get("sub")).unwrap_or_default(),
                size: int(entry.get("s")),
                grade: int(entry.get("g")),
                class: text(entry.get("c")),
                maker: text(entry.get("m")),
                ways: match entry.get("q") {
                    Some(Value::Array(items)) => items.iter().map(way).collect(),
                    _ => Vec::new(),
                },
                start: matches!(entry.get("start"), Some(Value::Bool(true))),
                pool: text(entry.get("topf")),
                since: text(entry.get("seit")),
                no_way: matches!(entry.get("ohne_weg"), Some(Value::Bool(true))),
                recipe_tag: text(entry.get("rezept_tag")).unwrap_or_default(),
                recipe_subtype: text(entry.get("rezept_unterart")).unwrap_or_default(),
                recipe_kind: text(entry.get("rezept_art")).unwrap_or_default(),
            })
            .collect(),
        _ => Vec::new(),
    };
    let missions = match root.get("missionen") {
        Some(Value::Object(map)) => map.iter().map(|(key, mission)| mission_from_value(key, mission)).collect(),
        _ => Vec::new(),
    };
    CatalogCache {
        version: text(root.get("version")).unwrap_or_default(),
        fetched: text(root.get("geholt")).unwrap_or_default(),
        blueprints,
        missions,
        reputation: None,
    }
}

fn put_text(map: &mut Map, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|v| !v.is_empty()) {
        map.push((key.into(), Value::str(value)));
    }
}

fn put_int(map: &mut Map, key: &str, value: Option<i64>) {
    if let Some(value) = value {
        map.push((key.into(), Value::int(value)));
    }
}

fn way_value(way: &Way) -> Value {
    let mut map = Map::new();
    put_text(&mut map, "auftrag", Some(&way.contract));
    put_text(&mut map, "typ", Some(&way.kind));
    put_text(&mut map, "fraktion", Some(&way.faction));
    put_int(&mut map, "uec", way.reward);
    put_int(&mut map, "ruf", way.reputation_gain);
    put_text(&mut map, "rang", way.rank.as_deref());
    put_int(&mut map, "rep", way.rank_reputation);
    if let Some(place) = &way.place {
        map.push((
            "wo".into(),
            Value::Object(vec![
                ("system".into(), Value::str(&place.system)),
                ("orte".into(), Value::Array(place.places.iter().map(Value::str).collect())),
                ("mehr".into(), Value::int(place.more)),
            ]),
        ));
    }
    Value::Object(map)
}

/// The catalog in the cache format, so that [`parse`] reads back the same
/// catalog: the inverse of [`parse`] for everything it reads.
///
/// A contract lists its blueprints by name (`parse` rebuilds the keys from
/// them); a key without a blueprint of that key is written as it is.
pub fn to_value(catalog: &CatalogCache) -> Value {
    let names: HashMap<&str, &str> =
        catalog.blueprints.iter().rev().map(|b| (b.key.as_str(), b.name.as_str())).collect();
    let blueprints = catalog
        .blueprints
        .iter()
        .map(|b| {
            let mut map = Map::new();
            map.push(("n".into(), Value::str(&b.name)));
            put_text(&mut map, "a", Some(&b.kind));
            put_text(&mut map, "sub", Some(&b.sub));
            put_int(&mut map, "s", b.size);
            put_int(&mut map, "g", b.grade);
            put_text(&mut map, "c", b.class.as_deref());
            put_text(&mut map, "m", b.maker.as_deref());
            if !b.ways.is_empty() {
                map.push(("q".into(), Value::Array(b.ways.iter().map(way_value).collect())));
            }
            if b.start {
                map.push(("start".into(), Value::Bool(true)));
            }
            put_text(&mut map, "topf", b.pool.as_deref());
            put_text(&mut map, "seit", b.since.as_deref());
            if b.no_way {
                map.push(("ohne_weg".into(), Value::Bool(true)));
            }
            put_text(&mut map, "rezept_tag", Some(&b.recipe_tag));
            put_text(&mut map, "rezept_unterart", Some(&b.recipe_subtype));
            put_text(&mut map, "rezept_art", Some(&b.recipe_kind));
            (b.key.clone(), Value::Object(map))
        })
        .collect();
    let missions = catalog
        .missions
        .iter()
        .enumerate()
        .map(|(i, mission)| {
            let mut own = Map::new();
            // Sorted by the names themselves, as 4.12 writes `sorted(all_names)`;
            // the keys are lower case and would sort `Cinch` before `CQ7`.
            let mut names: Vec<&str> =
                mission.blueprints.iter().map(|key| *names.get(key.as_str()).unwrap_or(&key.as_str())).collect();
            names.sort_unstable();
            own.push(("bp".into(), Value::Array(names.into_iter().map(Value::str).collect())));
            put_int(&mut own, "rep_max", mission.reputation_max);
            put_text(&mut own, "rang_max", mission.rank_max.as_deref());
            if let Some(shareable) = mission.shareable {
                own.push(("teilbar".into(), Value::Bool(shareable)));
            }
            put_int(&mut own, "cooldown", mission.cooldown);
            // The order 4.12 writes; what is not in that list comes last.
            let find = |map: &Map, key: &str| map.iter().find(|(k, _)| k == key).cloned();
            let mut map = Map::new();
            for key in MISSION_KEYS {
                map.extend(find(&own, key).or_else(|| find(&mission.extra, key)));
            }
            let unknown = mission.extra.iter().filter(|(k, _)| !MISSION_KEYS.contains(&k.as_str()));
            map.extend(unknown.cloned());
            (if mission.key.is_empty() { i.to_string() } else { mission.key.clone() }, Value::Object(map))
        })
        .collect();
    Value::Object(vec![
        ("version".into(), Value::str(&catalog.version)),
        ("geholt".into(), Value::str(&catalog.fetched)),
        ("bauplaene".into(), Value::Object(blueprints)),
        ("missionen".into(), Value::Object(missions)),
    ])
}

/// The catalog from the data folder; `None` if there is none or it is unreadable.
pub fn load(paths: &Paths) -> Option<CatalogCache> {
    let data = jsonio::read_json(&paths.app_file(CACHE_FILE)).ok()?;
    let catalog = parse(&data);
    (!catalog.blueprints.is_empty()).then_some(catalog)
}

/// The catalog the program works with now: the one of the game data if it
/// exists (see [`current_file`]), else the cache of 4.12; with the own
/// corrections of `bp-overrides.json` (see `overrides`).
pub fn load_current(paths: &Paths) -> Option<CatalogCache> {
    let data = jsonio::read_json(&current_file(paths)).ok()?;
    let mut catalog = parse(&data);
    crate::overrides::Overrides::load(paths).apply(&mut catalog);
    (!catalog.blueprints.is_empty()).then_some(catalog)
}

/// The grade as a letter: 1 = A … 4 = D.
pub fn grade_letter(grade: i64) -> Option<char> {
    (1..=26).contains(&grade).then(|| char::from(b'A' + (grade - 1) as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_cache_format() {
        let source = r#"{"version": "4.3.1-live", "geholt": "2026-10-15 23:37", "bauplaene": {
            "arcticstorm": {"n": "ArcticStorm", "a": "Cooler", "sub": "UNDEFINED", "s": 1, "g": 3,
                "c": "Civilian", "m": "Lightning Power Ltd.",
                "q": [{"auftrag": "Protect Fuel Tanks", "typ": "Mercenary", "fraktion": "Foxwell Enforcement",
                       "uec": 91750, "ruf": 100, "rang": "Contractor", "rep": 2200,
                       "wo": {"system": "Nyx", "orte": ["Nyx I", "Nyx II"], "mehr": 0}}]},
            "adp legs sienna": {"n": "ADP Legs Sienna", "ohne_weg": true, "a": "Char_Armor_Legs", "s": 1, "g": 1},
            "start": {"n": "Starter", "start": true, "q": [{"rang": null, "rep": null}]}
        }}"#;
        let catalog = parse(&jsonio::loads(source).unwrap());
        assert_eq!(catalog.version, "4.3.1-live");
        assert_eq!(catalog.blueprints.len(), 3);
        let cooler = &catalog.blueprints[0];
        assert_eq!((cooler.size, cooler.grade.and_then(grade_letter)), (Some(1), Some('C')));
        assert_eq!(cooler.class.as_deref(), Some("Civilian"));
        let way = &cooler.ways[0];
        assert_eq!(
            (way.reward, way.rank.as_deref(), way.rank_reputation),
            (Some(91750), Some("Contractor"), Some(2200))
        );
        assert_eq!(way.place.as_ref().map(|p| p.places.len()), Some(2));
        assert!(catalog.blueprints[1].ways.is_empty() && catalog.blueprints[1].class.is_none());
        let start = &catalog.blueprints[2];
        assert!(start.start);
        assert_eq!((start.ways[0].rank.clone(), start.ways[0].rank_reputation), (None, None));
    }

    #[test]
    fn written_catalog_reads_back_the_same() {
        let source = r#"{"version": "4.10.2-live.1", "geholt": "2026-10-15 23:37", "bauplaene": {
            "arcticstorm": {"n": "ArcticStorm", "a": "Cooler", "sub": "UNDEFINED", "s": 1, "g": 3,
                "c": "Civilian", "m": "Lightning Power Ltd.", "seit": "4.10.2-live.1",
                "q": [{"auftrag": "Protect Fuel Tanks", "typ": "Mercenary", "fraktion": "Foxwell Enforcement",
                       "uec": 91750, "ruf": 100, "rang": "Contractor", "rep": 2200,
                       "wo": {"system": "Nyx", "orte": ["Nyx I", "Nyx II"], "mehr": 2}}],
                "rezept_tag": "BP_CRAFT_COOL_LPLT_S01_ArcticStorm", "rezept_art": "cooler"},
            "s-38 magazine (20)": {"n": "S-38 Magazine (20 cap)", "start": true, "a": "ammo",
                "rezept_tag": "BP_CRAFT_behr_pistol_mag_01", "rezept_unterart": "pistol", "rezept_art": "ammo"},
            "xeno": {"n": "Xeno", "topf": "XenoThreat"},
            "adp legs sienna": {"n": "ADP Legs Sienna", "ohne_weg": true, "a": "Char_Armor_Legs"}
        }, "missionen": {"m1": {"bp": ["ArcticStorm", "S-38 Magazine (20 cap)"], "rep_max": 95250,
            "rang_max": "Elite Contractor", "teilbar": true, "cooldown": 45},
            "m2": {"bp": ["Gone"], "teilbar": false}}}"#;
        let catalog = parse(&jsonio::loads(source).unwrap());
        assert_eq!(catalog.missions[0].blueprints, ["arcticstorm", "s-38 magazine (20)"]);
        let magazine = &catalog.blueprints[1];
        assert_eq!(
            (magazine.recipe_tag.as_str(), magazine.recipe_subtype.as_str(), magazine.recipe_kind.as_str()),
            ("BP_CRAFT_behr_pistol_mag_01", "pistol", "ammo"),
            "the recipe is read"
        );
        let again = parse(&to_value(&catalog));
        assert_eq!(again, catalog);
    }

    #[test]
    fn the_other_values_of_a_contract_are_kept_in_the_order_of_4_12() {
        let source = r#"{"version": "4.10.2-live.1", "bauplaene": {"a": {"n": "A"}}, "missionen": {
            "m1": {"bp": ["A"], "sicher": true, "chance": 0.25, "rep": 800, "rang": "Jr. Contractor",
                "rep_max": 95250, "rang_max": "Elite Contractor", "uec": 70750, "ruf": 150, "teilbar": true,
                "cooldown": 45, "ab": {"A": {"rang": "Jr. Contractor", "rep": 800}}, "rep_eindeutig": true,
                "je_system": {"Stanton": ["A"]}, "leer": 1, "stufen": 3, "titel_key": "m1_title",
                "text_key": "m1_desc", "later": 7}}}"#;
        let catalog = parse(&jsonio::loads(source).unwrap());
        let mission = &catalog.missions[0];
        assert_eq!(mission.extra.len(), 14);
        assert_eq!(mission.extra[0], ("sicher".to_string(), Value::Bool(true)));
        assert_eq!(mission.extra[1], ("chance".to_string(), Value::Float(0.25)));
        // Written back: the order of 4.12, what 4.12 does not know last.
        let written = to_value(&catalog);
        let entry = written.get("missionen").and_then(|m| m.get("m1")).unwrap();
        let keys: Vec<&str> = entry.as_object().unwrap().iter().map(|(k, _)| k.as_str()).collect();
        let expected = MISSION_KEYS.iter().copied().chain(["later"]).collect::<Vec<_>>();
        assert_eq!(keys, expected);
        assert_eq!(parse(&written), catalog);
        // A contract without them writes only what it had.
        let bare = parse(&jsonio::loads(r#"{"missionen": {"m": {"bp": [], "teilbar": false}}}"#).unwrap());
        assert!(bare.missions[0].extra.is_empty());
        let written = to_value(&bare);
        let entry = written.get("missionen").and_then(|m| m.get("m")).and_then(Value::as_object).unwrap();
        let keys: Vec<&str> = entry.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["bp", "teilbar"]);
    }

    #[test]
    fn the_blueprints_of_a_contract_are_written_sorted_by_their_names() {
        let source = r#"{"bauplaene": {"cinch": {"n": "Cinch Scraper"}, "cq7": {"n": "CQ7 Rifle"}},
            "missionen": {"m": {"bp": []}}}"#;
        let mut catalog = parse(&jsonio::loads(source).unwrap());
        // As the archive gives them: by key, lower case.
        catalog.missions[0].blueprints = vec!["cinch scraper".into(), "cq7 rifle".into()];
        let written = to_value(&catalog);
        let bp = written.get("missionen").and_then(|m| m.get("m")).and_then(|m| m.get("bp")).unwrap();
        assert_eq!(bp, &Value::Array(vec![Value::str("CQ7 Rifle"), Value::str("Cinch Scraper")]));
    }
}
