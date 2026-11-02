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

//! The closed lists of the stock page, read from the files the Python
//! version keeps in the data folder (`crafting.py`, `mining.py`,
//! `quality_goods.py`, `places.py`, `selling.py`, `stock_catalog.py`,
//! `stock_kinds.py`).
//!
//! Only reads; a missing or unreadable file gives an empty list. Kinds come
//! from these files alone: item kinds only from the section of the goods
//! list.

use std::collections::{HashMap, HashSet};

use crate::jsonio::{self, Value};
use crate::text::{py_isspace, py_strip};

pub const RECIPES_FILE: &str = crate::recipe_file::FILE;
pub const MINING_FILE: &str = "mining-data.json";
pub const QUALITY_GOODS_FILE: &str = "waren-guete.json";
pub const PLACES_FILE: &str = "orte.json";
pub const SELLING_FILE: &str = "verkauf.json";
pub const CATALOG_FILE: &str = "lager-katalog.json";

const RECIPES_FORMAT: i64 = crate::recipe_file::FORMAT;
const MINING_FORMAT: i64 = 1;
const PLACES_FORMAT: i64 = 1;
const SELLING_FORMAT: i64 = 5;
const CATALOG_FORMAT: i64 = 1;
/// One bid this many times above the second highest is dropped.
const OUTLIER_FACTOR: f64 = 3.0;

const BUNDLED_CATALOG: &str = include_str!("../../../daten/lager-katalog.json");

/// Kinds of a material and of an item (`stock_kinds`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Ore,
    Refined,
    Mineral,
    Plant,
    Commodity,
    VehicleItem,
    VehicleWeapon,
    Weapon,
    WeaponAttachment,
    Armor,
    Clothing,
    Food,
    Collectible,
    Other,
}

pub const MATERIAL_KINDS: [Kind; 6] =
    [Kind::Ore, Kind::Refined, Kind::Mineral, Kind::Plant, Kind::Commodity, Kind::Other];
pub const ITEM_KINDS: [Kind; 9] = [
    Kind::VehicleItem,
    Kind::VehicleWeapon,
    Kind::Weapon,
    Kind::WeaponAttachment,
    Kind::Armor,
    Kind::Clothing,
    Kind::Food,
    Kind::Collectible,
    Kind::Other,
];

impl Kind {
    /// The stored name, as in `stock_kinds`.
    pub fn code(self) -> &'static str {
        match self {
            Kind::Ore => "ore",
            Kind::Refined => "refined",
            Kind::Mineral => "mineral",
            Kind::Plant => "plant",
            Kind::Commodity => "commodity",
            Kind::VehicleItem => "vehicle_item",
            Kind::VehicleWeapon => "vehicle_weapon",
            Kind::Weapon => "weapon",
            Kind::WeaponAttachment => "weapon_attachment",
            Kind::Armor => "armor",
            Kind::Clothing => "clothing",
            Kind::Food => "food",
            Kind::Collectible => "collectible",
            Kind::Other => "other",
        }
    }

    /// The text key of its display name.
    pub fn text_key(self) -> String {
        format!("s_ml_k_{}", self.code())
    }
}

/// One entry of the goods list (`stock_catalog.choices`).
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub name: String,
    pub item: Option<Value>,
    pub material: Option<Value>,
    pub section: String,
}

/// Everything the stock page reads besides its own two files.
#[derive(Clone, Debug, Default)]
pub struct StockData {
    material_names: Vec<String>,
    pieces: HashSet<String>,
    portions: HashMap<String, Vec<(f64, f64)>>,
    element_names: Vec<String>,
    minerals: HashSet<String>,
    plants: Vec<String>,
    quality_goods: Vec<(String, String)>,
    units: HashMap<String, String>,
    plant_aliases: HashMap<String, String>,
    plant_keys: HashSet<String>,
    storable: Vec<String>,
    places: Vec<String>,
    goods: HashMap<String, Vec<f64>>,
    choices: HashMap<String, Choice>,
    item_names: Vec<String>,
}

// ------------------------------------------------------------ helpers

/// Python's truth value of a JSON value.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) | Some(Value::Bool(false)) => false,
        Some(Value::Int(n)) => n != "0",
        Some(Value::Float(x)) => *x != 0.0,
        Some(Value::Str(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
        Some(Value::Bool(true)) => true,
    }
}

/// `data.get('format') == number` with Python's equality (`True == 1`).
pub(crate) fn format_is(data: &Value, number: i64) -> bool {
    match data.get("format") {
        Some(Value::Int(n)) => n.parse::<i64>().ok() == Some(number),
        Some(Value::Float(x)) => *x == number as f64,
        Some(Value::Bool(b)) => i64::from(*b) == number,
        _ => false,
    }
}

/// Python's `float(value)` for a JSON value, `None` where it raises.
pub(crate) fn py_float_value(v: &Value) -> Option<f64> {
    match v {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Int(n) => n.parse::<f64>().ok(),
        Value::Float(x) => Some(*x),
        Value::Str(s) => py_float(s),
        _ => None,
    }
}

/// The first code point of every block of ten Unicode decimal digits
/// (category Nd).
const DIGIT_ZEROS: [u32; 68] = [
    0x30, 0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66, 0xDE6, 0xE50, 0xED0,
    0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620,
    0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10, 0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0,
    0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x11F50,
    0x16A60, 0x16AC0, 0x16B50, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E4F0, 0x1E950,
    0x1FBF0,
];

/// The value of a decimal digit of any script (Python's `\d`).
pub fn decimal_digit(c: char) -> Option<u32> {
    let code = u32::from(c);
    DIGIT_ZEROS.iter().find(|zero| (**zero..**zero + 10).contains(&code)).map(|zero| code - zero)
}

/// Python's `float(text)`, `None` where it raises.
pub fn py_float(text: &str) -> Option<f64> {
    let t = py_strip(text);
    if t.is_empty() {
        return None;
    }
    let t: String = t.chars().map(|c| decimal_digit(c).and_then(|d| char::from_digit(d, 10)).unwrap_or(c)).collect();
    let chars: Vec<char> = t.chars().collect();
    let mut clean = String::with_capacity(t.len());
    for (i, c) in chars.iter().enumerate() {
        if *c == '_' {
            let before = i > 0 && chars[i - 1].is_ascii_digit();
            let after = chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if !(before && after) {
                return None;
            }
            continue;
        }
        clean.push(*c);
    }
    let lower = clean.to_ascii_lowercase();
    let body = lower.trim_start_matches(['+', '-']);
    let word = matches!(body, "inf" | "infinity" | "nan");
    if !word && !body.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | '+' | '-')) {
        return None;
    }
    clean.parse::<f64>().ok()
}

/// `(value or '')` as text, only for strings.
pub(crate) fn text_of(v: Option<&Value>) -> &str {
    match v {
        Some(Value::Str(s)) => s,
        _ => "",
    }
}

fn items(v: Option<&Value>) -> &[Value] {
    match v {
        Some(Value::Array(a)) => a,
        _ => &[],
    }
}

fn read(path: &str) -> Option<Value> {
    jsonio::read_json(path).ok()
}

/// Python's `str.lower` for sorting and comparing.
pub fn lower(text: &str) -> String {
    text.to_lowercase()
}

/// Python's `str.split()` without an argument.
pub fn py_split(text: &str) -> Vec<&str> {
    text.split(py_isspace).filter(|p| !p.is_empty()).collect()
}

/// Make material names comparable across sources (`crafting.norm_material`).
pub fn norm_material(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    let head = name.split('(').next().unwrap_or("");
    let mut short = lower(py_strip(head));
    if let Some(rest) = short.strip_prefix("raw ") {
        short = py_strip(rest).to_string();
    }
    short.replace("aluminium", "aluminum")
}

/// `HeartoftheWoods` becomes `Heart of the Woods` (`mining._readable`).
pub fn readable(joined: &str) -> String {
    let chars: Vec<char> = joined.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let mut hit = None;
        if i > 0 && chars[i - 1].is_ascii_lowercase() {
            for word in ["of", "the", "and"] {
                let w: Vec<char> = word.chars().collect();
                let end = i + w.len();
                if chars.len() > end && chars[i..end] == w[..] && chars[end].is_ascii_alphabetic() {
                    hit = Some(word);
                    break;
                }
            }
        }
        match hit {
            Some(word) => {
                out.push(' ');
                out.push_str(word);
                out.push(' ');
                i += word.len();
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    let spaced: Vec<char> = out.chars().collect();
    let mut text = String::new();
    for (i, c) in spaced.iter().enumerate() {
        if i > 0 && spaced[i - 1].is_ascii_lowercase() && c.is_ascii_uppercase() {
            text.push(' ');
        }
        text.push(*c);
    }
    py_split(&text).join(" ")
}

/// The tag `(Mil/3/A)` of a catalog entry (`stock_kinds.tag`): class, size from 1 to 99, grade from A to D.
pub fn item_tag(blueprint: &crate::catalog_cache::Blueprint) -> Option<String> {
    use crate::injection::specs;
    let size = blueprint.size.filter(|s| (1..=99).contains(s)).map(|s| s.to_string());
    let grade = blueprint.grade.filter(|g| (1..=4).contains(g)).and_then(crate::catalog_cache::grade_letter);
    let grade = grade.map(String::from);
    specs::tag_from_values(specs::class_short(blueprint.class.as_deref()), size.as_deref(), grade.as_deref())
}

/// The tag of every catalog entry that has one, by name key (`paths::name_key`); of two entries with the same
/// key and a tag the later one counts.
pub fn item_tags(catalog: &crate::catalog_cache::CatalogCache) -> HashMap<String, String> {
    catalog.blueprints.iter().filter_map(|b| item_tag(b).map(|tag| (b.key.clone(), tag))).collect()
}

/// `sorted(values, key=str.lower)`, stable.
fn sort_lower(values: &mut [String]) {
    values.sort_by_key(|v| lower(v));
}

// ------------------------------------------------------------ difflib

/// `difflib.SequenceMatcher(None, a, b).ratio()` (no junk; words stay far
/// below the 200 characters where `autojunk` starts).
pub fn ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let mut b2j: HashMap<char, Vec<usize>> = HashMap::new();
    for (j, c) in b.iter().enumerate() {
        b2j.entry(*c).or_default().push(j);
    }
    let longest = |alo: usize, ahi: usize, blo: usize, bhi: usize| -> (usize, usize, usize) {
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0usize);
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for (i, c) in a.iter().enumerate().take(ahi).skip(alo) {
            let mut next: HashMap<usize, usize> = HashMap::new();
            for &j in b2j.get(c).map(Vec::as_slice).unwrap_or(&[]) {
                if j < blo {
                    continue;
                }
                if j >= bhi {
                    break;
                }
                let k = j.checked_sub(1).and_then(|p| j2len.get(&p)).copied().unwrap_or(0) + 1;
                next.insert(j, k);
                if k > bestsize {
                    besti = i + 1 - k;
                    bestj = j + 1 - k;
                    bestsize = k;
                }
            }
            j2len = next;
        }
        while besti > alo && bestj > blo && a[besti - 1] == b[bestj - 1] {
            besti -= 1;
            bestj -= 1;
            bestsize += 1;
        }
        while besti + bestsize < ahi && bestj + bestsize < bhi && a[besti + bestsize] == b[bestj + bestsize] {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    };
    let mut matched = 0usize;
    let mut queue = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = queue.pop() {
        let (i, j, k) = longest(alo, ahi, blo, bhi);
        if k > 0 {
            matched += k;
            if alo < i && blo < j {
                queue.push((alo, i, blo, j));
            }
            if i + k < ahi && j + k < bhi {
                queue.push((i + k, ahi, j + k, bhi));
            }
        }
    }
    2.0 * matched as f64 / total as f64
}

/// `difflib.get_close_matches(word, possibilities, n, cutoff)`.
pub fn close_matches(word: &str, possibilities: &[String], n: usize, cutoff: f64) -> Vec<String> {
    let mut found: Vec<(f64, &String)> =
        possibilities.iter().map(|x| (ratio(x, word), x)).filter(|(r, _)| *r >= cutoff).collect();
    found.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| b.1.cmp(a.1)));
    found.into_iter().take(n).map(|(_, x)| x.clone()).collect()
}

// ------------------------------------------------------------ loading

impl StockData {
    /// A few names for the preview without own data.
    pub fn sample() -> StockData {
        let mut data = StockData {
            material_names: ["Agricium", "Hadanite", "Iron", "Laranite", "Taranite", "Titanium"]
                .map(String::from)
                .to_vec(),
            pieces: HashSet::from(["hadanite".to_string()]),
            places: ["Area18", "Levski", "Orison", "Seraphim Station"].map(String::from).to_vec(),
            goods: HashMap::from([
                ("Construction Materials".to_string(), vec![1450.0]),
                ("Laranite".to_string(), vec![2810.0, 3020.0]),
            ]),
            ..StockData::default()
        };
        data.build_aliases();
        data.build_storable();
        if let Some(catalog) = jsonio::loads(BUNDLED_CATALOG).ok().filter(|d| format_is(d, CATALOG_FORMAT)) {
            data.read_catalog(&catalog);
        }
        data
    }

    /// Read every list; `file` gives the full path of a file name.
    pub fn load(file: impl Fn(&str) -> String) -> StockData {
        let mut data = StockData::default();
        if let Some(recipes) = read(&file(RECIPES_FILE)).filter(|d| format_is(d, RECIPES_FORMAT)) {
            data.read_recipes(&recipes);
        }
        if let Some(mining) = read(&file(MINING_FILE)).filter(|d| format_is(d, MINING_FORMAT)) {
            data.read_mining(&mining);
        }
        if let Some(goods) = read(&file(QUALITY_GOODS_FILE)) {
            data.read_quality_goods(&goods);
        }
        data.build_aliases();
        data.build_storable();
        if let Some(places) = read(&file(PLACES_FILE)).filter(|d| format_is(d, PLACES_FORMAT)) {
            data.places = items(places.get("orte")).iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        }
        for own in crate::places::own_in(&file(crate::places::OWN_FILE)) {
            data.add_place(&own.name);
        }
        if let Some(selling) = read(&file(SELLING_FILE)).filter(|d| format_is(d, SELLING_FORMAT)) {
            data.read_selling(&selling);
        }
        let catalog = read(&file(CATALOG_FILE))
            .filter(|d| format_is(d, CATALOG_FORMAT))
            .or_else(|| jsonio::loads(BUNDLED_CATALOG).ok().filter(|d| format_is(d, CATALOG_FORMAT)));
        if let Some(catalog) = catalog {
            data.read_catalog(&catalog);
        }
        data
    }

    fn read_recipes(&mut self, recipes: &Value) {
        let mut names = HashSet::new();
        for blueprint in items(recipes.get("blueprints")) {
            for tier in items(blueprint.get("tiers")) {
                for slot in items(tier.get("slots")) {
                    for option in items(slot.get("options")) {
                        let material = match text_of(option.get("resourceName")) {
                            "" => text_of(option.get("itemName")),
                            name => name,
                        };
                        if option.get("type").and_then(Value::as_str) == Some("item") {
                            let item = text_of(option.get("itemName"));
                            if !item.is_empty() {
                                self.pieces.insert(norm_material(item));
                            }
                        }
                        if material.is_empty() {
                            continue;
                        }
                        names.insert(material.to_string());
                        let amount = option.get("quantity").and_then(py_float_value).unwrap_or(0.0);
                        let minimum = option.get("minQuality").and_then(py_float_value).unwrap_or(0.0);
                        if amount != 0.0 {
                            self.portions.entry(norm_material(material)).or_default().push((minimum, amount));
                        }
                    }
                }
            }
        }
        self.material_names = names.into_iter().collect();
        self.material_names.sort();
        sort_lower(&mut self.material_names);
    }

    /// Takes the materials of recipes read from the game data in place of the
    /// recipes file of 4.12: (material, counted in pieces, amount per piece,
    /// lowest quality). Names, pieces, portions and the storable list follow.
    pub fn add_recipe_materials(&mut self, lines: impl IntoIterator<Item = (String, bool, f64, f64)>) {
        let mut names: HashSet<String> = self.material_names.iter().cloned().collect();
        for (material, piece, amount, minimum) in lines {
            if material.is_empty() {
                continue;
            }
            if piece {
                self.pieces.insert(norm_material(&material));
            }
            if amount != 0.0 {
                self.portions.entry(norm_material(&material)).or_default().push((minimum, amount));
            }
            names.insert(material);
        }
        self.material_names = names.into_iter().collect();
        self.material_names.sort();
        sort_lower(&mut self.material_names);
        self.build_aliases();
        self.build_storable();
    }

    fn read_mining(&mut self, mining: &Value) {
        if let Some(Value::Object(elements)) = mining.get("elemente") {
            for (_, element) in elements {
                let name = text_of(element.get("name"));
                self.minerals.insert(norm_material(name));
                let stripped = py_strip(name);
                if !stripped.is_empty() {
                    self.element_names.push(stripped.to_string());
                }
            }
        }
        self.minerals.remove("");
        let mut plants = HashSet::new();
        for location in items(mining.get("locations")) {
            for group in items(location.get("groups")) {
                if group.get("groupName").and_then(Value::as_str) != Some("Harvestables") {
                    continue;
                }
                for deposit in items(group.get("deposits")) {
                    let name = py_strip(text_of(deposit.get("presetName")));
                    if let Some(rest) = name.strip_prefix("Plant ") {
                        plants.insert(readable(rest));
                    }
                }
            }
        }
        self.plants = plants.into_iter().collect();
        self.plants.sort();
        sort_lower(&mut self.plants);
    }

    fn read_quality_goods(&mut self, goods: &Value) {
        for entry in items(goods.get("waren")) {
            let Value::Array(parts) = entry else { continue };
            if !(parts.len() == 2 || parts.len() == 3) {
                continue;
            }
            let (Some(name), Some(unit)) = (parts[0].as_str(), parts[1].as_str()) else { continue };
            self.quality_goods.push((name.to_string(), unit.to_string()));
            self.units.insert(norm_material(name), unit.to_string());
        }
    }

    fn build_aliases(&mut self) {
        for plant in &self.plants {
            let first = py_split(plant).first().map(|w| lower(w)).unwrap_or_default();
            let hits: Vec<&String> = self
                .quality_goods
                .iter()
                .map(|(name, _)| name)
                .filter(|g| py_split(g).first().is_some_and(|w| lower(w) == first))
                .collect();
            if hits.len() == 1 && norm_material(hits[0]) != norm_material(plant) {
                self.plant_aliases.insert(norm_material(plant), hits[0].clone());
            }
        }
        self.plant_keys = self.plants.iter().map(|p| norm_material(p)).collect();
    }

    fn build_storable(&mut self) {
        let mut seen = HashSet::new();
        let mut values = Vec::new();
        let mut take = |name: &str| {
            if !name.is_empty() && seen.insert(norm_material(name)) {
                values.push(name.to_string());
            }
        };
        for name in &self.material_names {
            take(name);
        }
        for name in &self.element_names {
            take(name);
        }
        for (name, _) in &self.quality_goods {
            take(name);
        }
        for plant in &self.plants {
            if !self.plant_aliases.contains_key(&norm_material(plant)) {
                take(plant);
            }
        }
        sort_lower(&mut values);
        self.storable = values;
    }

    fn read_selling(&mut self, selling: &Value) {
        if let Some(Value::Object(goods)) = selling.get("waren") {
            for (name, rows) in goods {
                let prices =
                    items(Some(rows)).iter().map(|row| row.get("p").and_then(py_float_value).unwrap_or(0.0)).collect();
                self.goods.insert(name.clone(), prices);
            }
        }
    }

    fn read_catalog(&mut self, catalog: &Value) {
        for row in items(catalog.get("teile")) {
            let Value::Array(parts) = row else { continue };
            if parts.len() != 3 {
                continue;
            }
            let name = parts[0].as_str().unwrap_or("").to_string();
            let section = parts[2].as_str().unwrap_or("").to_string();
            let entry = self.choices.entry(lower(&name)).or_insert_with(|| Choice {
                name: name.clone(),
                item: None,
                material: None,
                section: section.clone(),
            });
            if entry.item.is_none() {
                entry.item = Some(parts[1].clone());
                entry.section = section;
            }
        }
        for row in items(catalog.get("waren")) {
            let Value::Array(parts) = row else { continue };
            if parts.len() != 2 {
                continue;
            }
            let name = parts[0].as_str().unwrap_or("").to_string();
            let entry = self.choices.entry(lower(&name)).or_insert_with(|| Choice {
                name: name.clone(),
                item: None,
                material: None,
                section: String::new(),
            });
            if entry.material.is_none() {
                entry.material = Some(parts[1].clone());
            }
        }
        self.item_names = self.choices.values().filter(|c| c.item.is_some()).map(|c| c.name.clone()).collect();
        self.item_names.sort_by_cached_key(|n| lower(n));
    }

    // -------------------------------------------------------- materials

    /// Every material in a recipe, sorted without case (`material_names`).
    pub fn material_names(&self) -> &[String] {
        &self.material_names
    }

    /// Everything that may be stored (`crafting.storable`).
    pub fn storable(&self) -> &[String] {
        &self.storable
    }

    /// Counted in pieces instead of SCU? (`crafting.is_piece`)
    pub fn is_piece(&self, material: &str) -> bool {
        let key = norm_material(material);
        if self.pieces.contains(&key) || self.plant_keys.contains(&key) {
            return true;
        }
        let name = self.plant_aliases.get(&key).map_or(material, String::as_str);
        self.units.get(&norm_material(name)).is_some_and(|u| u == "stueck")
    }

    /// The binding spelling for the stock, or `None` (`crafting.storage_name`).
    pub fn storage_name(&self, given: &str) -> Option<String> {
        let wanted = norm_material(given);
        if wanted.is_empty() {
            return None;
        }
        self.storable
            .iter()
            .find(|n| norm_material(n) == wanted)
            .cloned()
            .or_else(|| self.plant_aliases.get(&wanted).cloned())
    }

    /// Suggestions for an unknown name (`crafting.similar_materials`).
    pub fn similar_materials(&self, name: &str, most: usize) -> Vec<String> {
        let text = lower(py_strip(name));
        if text.is_empty() {
            return Vec::new();
        }
        let hits: Vec<String> =
            self.material_names.iter().filter(|n| lower(n).contains(&text)).take(most).cloned().collect();
        if !hits.is_empty() {
            return hits;
        }
        close_matches(&text, &self.material_names, most, 0.6)
    }

    /// The smallest amount a recipe asks for at this quality, or `None`
    /// (`crafting.smallest_portion`).
    pub fn smallest_portion(&self, material: &str, quality: f64) -> Option<f64> {
        self.portions
            .get(&norm_material(material))?
            .iter()
            .filter(|(minimum, _)| *minimum <= quality)
            .map(|(_, amount)| *amount)
            .reduce(f64::min)
    }

    /// Pieces first, then SCU goods (`materials.piece_first`).
    pub fn piece_first(&self, material: &str) -> u8 {
        if self.is_piece(material) { 0 } else { 1 }
    }

    // -------------------------------------------------------- places, goods

    /// Every known storage location (`places.all_places`).
    pub fn places(&self) -> &[String] {
        &self.places
    }

    /// Takes a place the player added into the list, sorted without case. Without a list nothing is added:
    /// an empty list allows every place.
    pub fn add_place(&mut self, name: &str) {
        let name = py_strip(name);
        if self.places.is_empty() || name.is_empty() || self.places.iter().any(|p| lower(p) == lower(name)) {
            return;
        }
        self.places.push(name.to_string());
        self.places.sort_by(|a, b| lower(a).cmp(&lower(b)).then_with(|| a.cmp(b)));
    }

    /// Takes a place out of the list (without case).
    pub fn remove_place(&mut self, name: &str) {
        let wanted = lower(py_strip(name));
        self.places.retain(|p| lower(p) != wanted);
    }

    /// Does this location exist? Empty and a missing list allow everything
    /// (`places.knows`).
    pub fn knows_place(&self, name: &str) -> bool {
        let wanted = lower(py_strip(name));
        if wanted.is_empty() || self.places.is_empty() {
            return true;
        }
        self.places.iter().any(|o| lower(o) == wanted)
    }

    /// The binding spelling of a location: `Some("")` for empty, `None`
    /// when unknown (`places.official_name`).
    pub fn official_place(&self, given: &str) -> Option<String> {
        let wanted = lower(py_strip(given));
        if wanted.is_empty() {
            return Some(String::new());
        }
        self.places.iter().find(|o| lower(o) == wanted).cloned()
    }

    /// Every commodity with a sell offer, sorted (`selling.goods`).
    pub fn goods(&self) -> Vec<String> {
        let mut names: Vec<String> = self.goods.keys().cloned().collect();
        names.sort();
        names
    }

    /// Exact match against the commodity list (`selling.known`).
    pub fn knows_goods(&self, name: &str) -> bool {
        self.goods.contains_key(name)
    }

    /// The highest bid per SCU without one absurd outlier, or `0.0`
    /// (`selling.best_price`).
    pub fn best_price(&self, name: &str) -> f64 {
        let Some(prices) = self.goods.get(name) else { return 0.0 };
        let mut values = prices.clone();
        values.sort_by(f64::total_cmp);
        let mut rows: Vec<f64> = prices.clone();
        if values.len() >= 3 {
            let (second, highest) = (values[values.len() - 2], values[values.len() - 1]);
            if second > 0.0 && highest / second >= OUTLIER_FACTOR {
                rows.retain(|p| *p < highest);
            }
        }
        rows.into_iter().fold(0.0, f64::max)
    }

    /// The goods list by lower-case name (`stock_catalog.choices`).
    pub fn choices(&self) -> &HashMap<String, Choice> {
        &self.choices
    }

    /// The items of the goods list (`my_stock_page.item_choices`).
    pub fn item_names(&self) -> &[String] {
        &self.item_names
    }

    /// The item entry for a typed name, if it is in the list.
    pub fn item_choice(&self, name: &str) -> Option<&Choice> {
        self.choices.get(&lower(name)).filter(|c| c.item.is_some())
    }

    // -------------------------------------------------------- kinds

    /// The kind of a material (`stock_kinds.material_kind`, without the
    /// Basetool).
    pub fn material_kind(&self, name: &str, for_sale: bool) -> Kind {
        let text = py_strip(name);
        let low = lower(text);
        let key = norm_material(text);
        if low.contains("(ore)") || low.contains("(raw)") || low.starts_with("raw ") {
            return Kind::Ore;
        }
        if self.plant_keys.contains(&key) || self.plant_aliases.values().any(|g| norm_material(g) == key) {
            return Kind::Plant;
        }
        if self.pieces.contains(&key) {
            return Kind::Mineral;
        }
        if self.minerals.contains(&key) {
            return Kind::Refined;
        }
        if for_sale {
            return Kind::Commodity;
        }
        if self.choices.get(&low).is_some_and(|c| c.material.is_some()) {
            return Kind::Commodity;
        }
        Kind::Other
    }

    /// The kind of a material with the `materialKind` the Basetool sent
    /// (`stock_kinds.material_kind` with `server`); without one, or with a
    /// type it does not route, as [`StockData::material_kind`].
    ///
    /// ⚠ The Basetool sets `harvestable` also for minerals mined by hand or
    /// vehicle, so the own lists decide first and `harvestable` counts only
    /// for names none of them knows.
    pub fn material_kind_with(&self, name: &str, server: Option<&Value>) -> Kind {
        let key = norm_material(py_strip(name));
        if let Some(kind @ Value::Object(_)) = server {
            let item_type = kind.get("type").and_then(Value::as_str).unwrap_or_default().to_uppercase();
            let plant = self.plant_keys.contains(&key) || self.plant_aliases.values().any(|g| norm_material(g) == key);
            let found = match item_type.as_str() {
                "REFINED" => Some(Kind::Refined),
                "RAW" | "NO_REFINE" => Some(if plant {
                    Kind::Plant
                } else if self.pieces.contains(&key) {
                    Kind::Mineral
                } else if self.minerals.contains(&key) {
                    Kind::Ore
                } else if truthy(kind.get("harvestable")) {
                    Kind::Plant
                } else if item_type == "RAW" {
                    Kind::Ore
                } else if truthy(kind.get("commodity")) {
                    Kind::Commodity
                } else {
                    Kind::Other
                }),
                _ => None,
            };
            if let Some(found) = found {
                return found;
            }
        }
        self.material_kind(name, false)
    }

    /// The kind of an item from its section in the goods list
    /// (`stock_kinds.item_kind`, last source only).
    pub fn item_kind(&self, name: &str) -> Kind {
        let Some(entry) = self.choices.get(&lower(py_strip(name))).filter(|c| c.item.is_some()) else {
            return Kind::Other;
        };
        match entry.section.as_str() {
            "Systems" | "Propulsion" | "Avionics" | "Utility" | "Module" => Kind::VehicleItem,
            "Vehicle Weapons" => Kind::VehicleWeapon,
            "Personal Weapons" => Kind::Weapon,
            "Armor" | "Undersuits" => Kind::Armor,
            "Clothing" => Kind::Clothing,
            "Miscellaneous" => Kind::Collectible,
            _ => Kind::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_basetool_kind_goes_first_but_the_own_lists_decide_raw() {
        let mut data = StockData::sample();
        data.minerals.insert("laranite".into());
        data.plant_keys.insert("pitambu".into());
        let kind = |text: &str| jsonio::loads(text).ok();
        let check = |name: &str, server: &str, want: Kind| {
            assert_eq!(data.material_kind_with(name, kind(server).as_ref()), want, "{name} {server}");
        };
        check("Laranite", r#"{"type": "REFINED"}"#, Kind::Refined);
        check("Laranite", r#"{"type": "raw", "harvestable": true}"#, Kind::Ore);
        check("Hadanite", r#"{"type": "RAW", "harvestable": true}"#, Kind::Mineral);
        check("Pitambu", r#"{"type": "NO_REFINE"}"#, Kind::Plant);
        check("Neu", r#"{"type": "NO_REFINE", "harvestable": true}"#, Kind::Plant);
        check("Neu", r#"{"type": "RAW"}"#, Kind::Ore);
        check("Neu", r#"{"type": "NO_REFINE", "commodity": true}"#, Kind::Commodity);
        check("Neu", r#"{"type": "NO_REFINE"}"#, Kind::Other);
        // An unknown type or no object: as without the Basetool.
        check("Laranite", r#"{"type": "ANDERS"}"#, Kind::Refined);
        check("Gold (Ore)", r#""kein Objekt""#, Kind::Ore);
        assert_eq!(data.material_kind_with("Hadanite", None), Kind::Mineral);
    }

    #[test]
    fn an_item_tag_needs_two_of_class_size_and_grade_or_a_weapon_class() {
        use crate::catalog_cache::Blueprint;
        let entry = |class: Option<&str>, size: Option<i64>, grade: Option<i64>| Blueprint {
            class: class.map(String::from),
            size,
            grade,
            ..Blueprint::default()
        };
        assert_eq!(item_tag(&entry(Some("Military"), Some(3), Some(1))).as_deref(), Some("(Mil/3/A)"));
        assert_eq!(item_tag(&entry(None, Some(2), Some(4))).as_deref(), Some("(–/2/D)"));
        assert_eq!(item_tag(&entry(Some("Ballistic"), None, None)).as_deref(), Some("(Bal)"));
        assert_eq!(item_tag(&entry(Some("Civilian"), None, None)), None, "one value alone is no tag");
        // Out of range counts as missing, as in 4.12: grade 5 and size 0 or 100.
        assert_eq!(item_tag(&entry(Some("Stealth"), Some(0), Some(5))), None);
        assert_eq!(item_tag(&entry(Some("Stealth"), Some(100), Some(2))).as_deref(), Some("(Sth/–/B)"));
        let catalog = crate::catalog_cache::CatalogCache {
            blueprints: vec![
                Blueprint { key: "fr-66".into(), ..entry(Some("Military"), Some(2), Some(1)) },
                Blueprint { key: "helm".into(), ..entry(Some("Civilian"), None, None) },
            ],
            ..Default::default()
        };
        let tags = item_tags(&catalog);
        assert_eq!(tags.get("fr-66").map(String::as_str), Some("(Mil/2/A)"));
        assert!(!tags.contains_key("helm"));
    }

    #[test]
    fn norm_material_like_python() {
        assert_eq!(norm_material("Agricium (Ore)"), "agricium");
        assert_eq!(norm_material("Raw Ouratite"), "ouratite");
        assert_eq!(norm_material(" Aluminium "), "aluminum");
        assert_eq!(norm_material(""), "");
    }

    #[test]
    fn readable_splits_joined_names() {
        assert_eq!(readable("HeartoftheWoods"), "Heart of the Woods");
        assert_eq!(readable("SunsetBerry"), "Sunset Berry");
        assert_eq!(readable("Decari"), "Decari");
    }

    #[test]
    fn ratio_like_difflib() {
        assert!((ratio("aslerite", "Aslarite") - 0.75).abs() < 1e-9);
        assert!((ratio("abcd", "bcde") - 0.75).abs() < 1e-9);
        assert_eq!(ratio("", ""), 1.0);
    }

    #[test]
    fn py_float_accepts_python_spellings() {
        assert_eq!(py_float(" 1_000.5 "), Some(1000.5));
        assert_eq!(py_float("1__0"), None);
        assert_eq!(py_float("-inf"), Some(f64::NEG_INFINITY));
        assert_eq!(py_float("12,5"), None);
        assert_eq!(py_float(""), None);
    }

    #[test]
    fn best_price_drops_one_outlier() {
        let mut data = StockData::default();
        data.goods.insert("Gold".into(), vec![100.0, 110.0, 900.0]);
        data.goods.insert("Iron".into(), vec![100.0, 250.0]);
        assert_eq!(data.best_price("Gold"), 110.0);
        assert_eq!(data.best_price("Iron"), 250.0);
        assert_eq!(data.best_price("Nichts"), 0.0);
    }
}
