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

//! Where which ore lies, read from the mining data (`mining-data.json`,
//! `mining.locations`, `ores`, `material_key`, `in_game_name` of 4.12).
//!
//! The share of an ore at a place is a share of the chunks, not a yield: the
//! weight of an ore is the probability of its group, times the share of its
//! deposit in the group, times its probability in the composition, times the
//! middle of its percentage; per way of mining (hand, vehicle, ship) the
//! weights are brought to 100 %. An ore that comes in several ways keeps the
//! higher share.

use std::collections::{BTreeSet, HashMap};

use crate::jsonio::{self, Value};
use crate::language::t;
use crate::stock_data::{lower, norm_material, py_float_value, text_of};

/// The file of the mining data in the data folder.
pub const FILE: &str = "mining-data.json";

/// Which group of the data is which way of mining; any other is no ore.
const KINDS: [(&str, &str); 4] = [
    ("FPS_Mineables", "fps"),
    ("SpaceShip_Mineables", "schiff"),
    ("SpaceShip_Mineables_Rare", "schiff_selten"),
    ("GroundVehicle_Mineables", "fahrzeug"),
];

/// Some places in the data are templates, not places: the points of the game
/// that use them.
const IN_GAME_NAMES: [(&str, &str); 10] = [
    ("hpp_lagrange_a", "HUR L1 · HUR L4"),
    ("hpp_lagrange_b", "ARC L5 · CRU L4 · MIC L3"),
    ("hpp_lagrange_c", "CRU L3 · HUR L5 · MIC L1 · MIC L2 · MIC L5"),
    ("hpp_lagrange_d", "ARC L3 · CRU L5 · MIC L4"),
    ("hpp_lagrange_e", "CRU L1 · CRU L2 · HUR L3"),
    ("hpp_lagrange_f", "ARC L1 · ARC L2 · ARC L4 · HUR L2"),
    ("hpp_pyro_cool01", "PYR5 L1–L5"),
    ("hpp_pyro_cool02", "PYR6 L1–L5"),
    ("hpp_pyro_warm01", "PYR1 L1–L5 · PYR2 L1–L5"),
    ("hpp_pyro_warm02", "PYR3 L1–L5"),
];

/// Templates without a fixed place keep their name and get a note.
const IN_GAME_HINTS: [(&str, &str); 5] = [
    ("hpp_lagrange_g", "bg_ort_ohne"),
    ("hpp_lagrange_occupied", "bg_ort_ohne"),
    ("asteroidcluster_low_yield", "bg_ort_claim"),
    ("asteroidcluster_medium_yield", "bg_ort_claim"),
    ("hpp_pyro_deepspaceasteroids", "bg_ort_tiefraum"),
];

/// The six bands of a share, in percent.
const LEVELS: [(i64, u8); 5] = [(50, 6), (30, 5), (15, 4), (7, 3), (2, 2)];

/// 1 to 6, from "hardly anything" to "almost only that", on the rounded percent.
pub fn level(share: f64) -> u8 {
    let percent = (share * 100.0).round_ties_even() as i64;
    LEVELS.iter().find(|(threshold, _)| percent >= *threshold).map_or(1, |(_, value)| *value)
}

/// The share of an ore with one tool at one place: the share, its level and how many ores that tool finds
/// there at all (one alone is "the only one here", not "100 %").
#[derive(Clone, Debug, PartialEq)]
pub struct ToolShare {
    /// `schiff`, `fahrzeug` or `fps`: the rare ship rocks count with the ship.
    pub tool: String,
    pub share: f64,
    pub level: u8,
    pub ores_there: usize,
}

/// One place where an ore lies.
#[derive(Clone, Debug, PartialEq)]
pub struct OrePlace {
    /// The name the player finds in the game.
    pub name: String,
    pub system: String,
    /// The ways of mining it there.
    pub kinds: BTreeSet<String>,
    /// The share of the chunks, from 0 to 1: the higher one over the tools.
    pub share: f64,
    pub level: u8,
    /// The share per tool, as it was computed.
    pub per_tool: Vec<ToolShare>,
}

/// One ore with its places, the richest first.
#[derive(Clone, Debug, PartialEq)]
pub struct Ore {
    pub name: String,
    pub places: Vec<OrePlace>,
}

/// One ore at a place.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaceOre {
    pub name: String,
    pub kinds: BTreeSet<String>,
    pub share: f64,
    pub level: u8,
}

/// The ores of one tool at a place, each with its share and level.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolOres {
    pub tool: String,
    pub ores: Vec<(String, f64, u8)>,
}

/// One place with ores (`mining.locations` of 4.12).
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    /// The name the player finds in the game.
    pub name: String,
    /// The name in the data (a template stands under the name of its points in the game).
    pub template: String,
    pub system: String,
    /// The kind of place (`locationType`).
    pub kind: String,
    pub ores: Vec<PlaceOre>,
    pub per_tool: Vec<ToolOres>,
}

/// A raw material with its scan signature.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub name: String,
    /// The signature of one chunk; 0 where the data has none.
    pub signature: f64,
    pub rarity: String,
}

/// A refinery and the profile of its bonuses.
#[derive(Clone, Debug, PartialEq)]
pub struct Refinery {
    pub name: String,
    pub system: String,
    pub profile: String,
}

/// The mining data, evaluated.
#[derive(Clone, Debug, Default)]
pub struct MiningData {
    ores: Vec<Ore>,
    places: Vec<Place>,
    aliases: HashMap<String, String>,
    elements: Vec<Element>,
    refineries: Vec<Refinery>,
    /// Per profile the bonus per material as it stands in the data.
    profiles: Vec<(String, Vec<(String, Value)>)>,
    build: String,
    /// The asteroid types the game scans (`Asteroid (C-Type)` …) with the signature of one, from the game data.
    asteroids: Vec<(String, f64)>,
}

/// The file the signatures of the rocks read from the game data are kept in, next to [`FILE`].
pub const ROCK_SIGNATURES_FILE: &str = "signaturen-spieldaten.json";
/// A rock whose name starts so is an asteroid type of the game data (`Asteroid (C-Type)`), no raw material.
const ASTEROID_PREFIX: &str = "Asteroid (";
/// The rarity an asteroid type counts as for how many rocks the scanner shows at once ([`max_chunks`]): the
/// game data state no rarity for it, so it counts like a common ore.
pub const ASTEROID_RARITY: &str = "common";

fn items(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(py_float_value)
}

fn name_in_game(location: &Value) -> String {
    let original = match text_of(location.get("locationName")) {
        "" => "?".to_string(),
        name => name.to_string(),
    };
    let preset = text_of(location.get("presetFile"));
    if let Some((_, name)) = IN_GAME_NAMES.iter().find(|(key, _)| *key == preset) {
        return (*name).to_string();
    }
    if let Some((_, key)) = IN_GAME_HINTS.iter().find(|(known, _)| *known == preset) {
        return format!("{original} ({})", t(key));
    }
    original
}

/// What lies at one place: the ways of mining per ore and the share per ore
/// in the order the ores first appear, and the shares per tool as computed.
type AtPlace = (Vec<(String, BTreeSet<String>, f64)>, Vec<(String, Vec<(String, f64)>)>);

fn at_location(location: &Value, compositions: Option<&Value>) -> AtPlace {
    let pot = |kind: &str| if kind.starts_with("schiff") { "schiff".to_string() } else { kind.to_string() };
    let mut kinds: Vec<(String, BTreeSet<String>)> = Vec::new();
    let mut weight: Vec<((String, String), f64)> = Vec::new();
    for group in items(location.get("groups")) {
        let Some((_, kind)) = KINDS.iter().find(|(name, _)| *name == text_of(group.get("groupName"))) else {
            continue;
        };
        let group_probability = number(group.get("groupProbability")).unwrap_or(1.0);
        let deposits = items(group.get("deposits"));
        // `sum()` of Python 3.12 and later is compensated: the last bit of a share depends on it.
        let total = crate::text::py_sum(deposits.iter().map(|d| number(d.get("relativeProbability")).unwrap_or(0.0)));
        for deposit in deposits {
            let guid = text_of(deposit.get("compositionGuid"));
            let Some(composition) = compositions.and_then(|c| c.get(guid)).filter(|c| !matches!(c, Value::Null)) else {
                continue;
            };
            let deposit_share = if total != 0.0 {
                number(deposit.get("relativeProbability")).unwrap_or(0.0) / total
            } else {
                1.0 / deposits.len().max(1) as f64
            };
            for part in items(composition.get("parts")) {
                let name = text_of(part.get("elementName"));
                if name.is_empty() {
                    continue;
                }
                match kinds.iter_mut().find(|(n, _)| n == name) {
                    Some((_, set)) => {
                        set.insert((*kind).to_string());
                    }
                    None => kinds.push((name.to_string(), BTreeSet::from([(*kind).to_string()]))),
                }
                let middle = (number(part.get("minPercent")).unwrap_or(0.0)
                    + number(part.get("maxPercent")).unwrap_or(0.0))
                    / 2.0;
                let probability = number(part.get("probability")).unwrap_or(1.0);
                let key = (pot(kind), name.to_string());
                let add = group_probability * deposit_share * probability * middle;
                match weight.iter_mut().find(|(k, _)| *k == key) {
                    Some((_, sum)) => *sum += add,
                    None => weight.push((key, add)),
                }
            }
        }
    }
    let mut totals: Vec<(String, f64)> = Vec::new();
    for ((pot, _), w) in &weight {
        match totals.iter_mut().find(|(p, _)| p == pot) {
            Some((_, sum)) => *sum += w,
            None => totals.push((pot.clone(), *w)),
        }
    }
    let mut shares: Vec<(String, f64)> = kinds.iter().map(|(n, _)| (n.clone(), 0.0)).collect();
    let mut per_tool: Vec<(String, Vec<(String, f64)>)> = Vec::new();
    for ((pot, name), w) in &weight {
        let pot_total = totals.iter().find(|(p, _)| p == pot).map_or(0.0, |(_, s)| *s);
        let value = if pot_total != 0.0 { w / pot_total } else { 0.0 };
        match per_tool.iter_mut().find(|(p, _)| p == pot) {
            Some((_, list)) => list.push((name.clone(), value)),
            None => per_tool.push((pot.clone(), vec![(name.clone(), value)])),
        }
        if let Some((_, best)) = shares.iter_mut().find(|(n, _)| n == name) {
            *best = best.max(value);
        }
    }
    let ores = kinds
        .into_iter()
        .map(|(name, set)| {
            let share = shares.iter().find(|(n, _)| *n == name).map_or(0.0, |(_, s)| *s);
            (name, set, share)
        })
        .collect();
    (ores, per_tool)
}

impl MiningData {
    /// Reads the file; no data without a usable one. The signatures the game data gave
    /// ([`ROCK_SIGNATURES_FILE`] beside the file) win over the file where they are there.
    pub fn load(file: &str) -> MiningData {
        match jsonio::read_json(file) {
            Ok(data) if crate::stock_data::format_is(&data, 1) => {
                let mut mining = MiningData::of(&data);
                if let Some(folder) = std::path::Path::new(file).parent()
                    && let Ok(rocks) = jsonio::read_json(&folder.join(ROCK_SIGNATURES_FILE).to_string_lossy())
                {
                    mining.apply_rocks(&rocks);
                }
                mining
            }
            _ => MiningData::default(),
        }
    }

    /// Takes the signatures of the rocks from the game data (`{"format": 1, "steine": [{"name", "signatur"}]}`):
    /// a raw material of that name that has a signature gets the one the game data states, unless the rocks of
    /// that name state several; an asteroid type is added. Anything else is left out: a deposit of the ground or
    /// of hand mining is no raw material of its own, its signature is one of the [`base_signatures`].
    pub fn apply_rocks(&mut self, rocks: &Value) {
        if !crate::stock_data::format_is(rocks, 1) {
            return;
        }
        let mut by_name: Vec<(String, Vec<f64>)> = Vec::new();
        for rock in items(rocks.get("steine")) {
            let (name, Some(signature)) = (text_of(rock.get("name")), number(rock.get("signatur"))) else { continue };
            if name.is_empty() || signature <= 0.0 {
                continue;
            }
            match by_name.iter_mut().find(|(n, _)| n == name) {
                Some((_, known)) if !known.contains(&signature) => known.push(signature),
                Some(_) => {}
                None => by_name.push((name.to_string(), vec![signature])),
            }
        }
        for (name, signatures) in by_name {
            let [signature] = signatures[..] else { continue };
            if let Some(element) = self.elements.iter_mut().find(|e| e.name == name && e.signature != 0.0) {
                element.signature = signature;
            } else if name.starts_with(ASTEROID_PREFIX) && !self.asteroids.iter().any(|(n, _)| *n == name) {
                self.asteroids.push((name, signature));
            }
        }
        self.asteroids.sort_by(|a, b| a.0.cmp(&b.0));
    }

    /// The deposits without a raw material of their own that carry a scan signature: name, signature of one, most
    /// at once ([`base_signatures`] and the asteroid types of the game data).
    pub fn deposits(&self) -> Vec<(String, f64, u32)> {
        base_signatures()
            .iter()
            .map(|&(name, signature, most)| (name.to_string(), signature, most))
            .chain(
                self.asteroids
                    .iter()
                    .map(|(name, signature)| (name.clone(), *signature, max_chunks(ASTEROID_RARITY))),
            )
            .collect()
    }

    /// The data of a loaded file.
    pub fn of(data: &Value) -> MiningData {
        let compositions = data.get("compositions");
        // Per place in the order of the names in the game.
        let mut places: Vec<Place> = Vec::new();
        for location in items(data.get("locations")) {
            let (ores, per_tool) = at_location(location, compositions);
            if ores.is_empty() {
                continue;
            }
            places.push(Place {
                name: name_in_game(location),
                template: text_of(location.get("locationName")).to_string(),
                system: text_of(location.get("system")).to_string(),
                kind: text_of(location.get("locationType")).to_string(),
                ores: ores
                    .into_iter()
                    .map(|(name, kinds, share)| PlaceOre { name, kinds, share, level: level(share) })
                    .collect(),
                per_tool: per_tool
                    .into_iter()
                    .map(|(tool, list)| ToolOres {
                        tool,
                        ores: list.into_iter().map(|(name, share)| (name, share, level(share))).collect(),
                    })
                    .collect(),
            });
        }
        places.sort_by_cached_key(|p| lower(&p.name));
        let mut collected: Vec<(String, Vec<OrePlace>)> = Vec::new();
        for place in &places {
            for ore in &place.ores {
                // Per tool the share, and how many ores the tool finds there at all.
                let per_tool = place
                    .per_tool
                    .iter()
                    .filter_map(|t| {
                        let (_, share, level) = t.ores.iter().find(|(n, _, _)| *n == ore.name)?;
                        Some(ToolShare { tool: t.tool.clone(), share: *share, level: *level, ores_there: t.ores.len() })
                    })
                    .collect();
                let entry = OrePlace {
                    name: place.name.clone(),
                    system: place.system.clone(),
                    kinds: ore.kinds.clone(),
                    share: ore.share,
                    level: ore.level,
                    per_tool,
                };
                match collected.iter_mut().find(|(n, _)| *n == ore.name) {
                    Some((_, list)) => list.push(entry),
                    None => collected.push((ore.name.clone(), vec![entry])),
                }
            }
        }
        let mut ores: Vec<Ore> = collected
            .into_iter()
            .map(|(name, mut list)| {
                list.sort_by(|a, b| b.share.total_cmp(&a.share).then_with(|| lower(&a.name).cmp(&lower(&b.name))));
                Ore { name, places: list }
            })
            .collect();
        ores.sort_by_cached_key(|o| lower(&o.name));
        let elements = match data.get("elemente") {
            Some(Value::Object(elements)) => elements
                .iter()
                .map(|(_, e)| Element {
                    name: text_of(e.get("name")).to_string(),
                    signature: number(e.get("scanSignature")).unwrap_or(0.0),
                    rarity: text_of(e.get("rarity")).to_string(),
                })
                .collect(),
            _ => Vec::new(),
        };
        let refineries = items(data.get("refineries"))
            .iter()
            .filter_map(|r| {
                // A refinery without a profile in the data has no column.
                let profile = r.get("profileId").and_then(Value::as_str)?;
                Some(Refinery {
                    name: text_of(r.get("name")).to_string(),
                    system: text_of(r.get("system")).to_string(),
                    profile: profile.to_string(),
                })
            })
            .collect();
        let profiles = match data.get("refineryProfiles") {
            Some(Value::Object(profiles)) => profiles
                .iter()
                .map(|(id, values)| {
                    let values = match values {
                        Value::Object(values) => values.clone(),
                        _ => Vec::new(),
                    };
                    (id.clone(), values)
                })
                .collect(),
            _ => Vec::new(),
        };
        let build = match data.get("build") {
            Some(Value::Str(build)) => build.clone(),
            Some(Value::Int(build)) => build.clone(),
            _ => String::new(),
        };
        MiningData {
            ores,
            places,
            aliases: aliases_of(data),
            elements,
            refineries,
            profiles,
            build,
            asteroids: Vec::new(),
        }
    }

    /// Every ore with its places, the richest first.
    pub fn ores(&self) -> &[Ore] {
        &self.ores
    }

    /// Every place with ores, by name.
    pub fn places(&self) -> &[Place] {
        &self.places
    }

    /// The raw materials with their scan signatures, in the order of the data.
    pub fn elements(&self) -> &[Element] {
        &self.elements
    }

    /// The refineries that have a profile name, in the order of the data.
    pub fn refineries(&self) -> &[Refinery] {
        &self.refineries
    }

    /// Per profile the bonus per material, as it stands in the data.
    pub fn profiles(&self) -> &[(String, Vec<(String, Value)>)] {
        &self.profiles
    }

    /// The game build of the data; empty where the file names none.
    pub fn build(&self) -> &str {
        &self.build
    }

    /// Whether there is anything to show.
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// The comparable key of a raw material across the three names it goes
    /// by (`Pressurized Ice`, `Ice (Raw)`, `Raw Ice`).
    pub fn material_key(&self, name: &str) -> String {
        let key = norm_material(name);
        self.aliases.get(&key).cloned().unwrap_or(key)
    }

    /// How a raw material is mined over all its places: `fps`, `fahrzeug`, `schiff`
    /// (`mining.mining_kinds`); the rare ship rocks count with the ship. Empty where the data do not know it.
    pub fn mining_kinds(&self, name: &str) -> BTreeSet<String> {
        let wanted = self.material_key(name);
        self.ores
            .iter()
            .filter(|ore| self.material_key(&ore.name) == wanted)
            .flat_map(|ore| ore.places.iter().flat_map(|place| place.kinds.iter()))
            .map(|kind| if kind.starts_with("schiff") { "schiff".to_string() } else { kind.clone() })
            .collect()
    }
}

// ------------------------------------------------------------ the scan signature

/// How many chunks a deposit of this rarity has at most (`mining.MAX_CHUNKS`); unknown counts as common.
pub fn max_chunks(rarity: &str) -> u32 {
    match rarity {
        "legendary" => 2,
        "epic" => 3,
        "rare" => 4,
        "uncommon" => 5,
        _ => 6,
    }
}

/// Deposits without a raw material of their own that the game scans: name, signature of one, most chunks. The
/// ground deposits (ROC, 4,000), the hand-mined minerals (`fps`, 3,000) and the salvage (2,000).
const BASE_SIGNATURES: [(&str, f64, u32); 3] = [("roc", 4000.0, 7), ("fps", 3000.0, 10), ("salvage", 2000.0, 15)];

/// The deposits without a raw material of their own that carry a scan signature.
pub fn base_signatures() -> &'static [(&'static str, f64, u32)] {
    &BASE_SIGNATURES
}

/// Whether a hit is one of the [`base_signatures`], not a raw material.
pub fn is_base_signature(name: &str) -> bool {
    BASE_SIGNATURES.iter().any(|(base, _, _)| *base == name)
}

/// One raw material that matches a scanned value.
#[derive(Clone, Debug, PartialEq)]
pub struct SignatureHit {
    pub name: String,
    pub chunks: u32,
    pub total: f64,
    /// How far the signature lies from the value, in percent.
    pub deviation: f64,
}

/// The value a typed signature stands for and the range it allows, or `None` for no usable number. As in the
/// game HUD with a thousands separator (`17,200` is 17200); `~5000` allows 10 % either way, `4000-9000` all
/// between.
pub fn signature_range(query: &str) -> Option<(f64, f64, f64)> {
    let text = crate::stock::normalize_separators(query, true);
    if text.is_empty() {
        return None;
    }
    if let Some(rest) = text.strip_prefix('~') {
        let value = crate::stock_data::py_float(rest)?;
        return Some((value, value * 0.9, value * 1.1));
    }
    let mut chars = text.chars();
    let first = chars.next()?;
    if chars.as_str().contains('-') {
        if first == '-' {
            return None;
        }
        let (a, b) = text.split_once('-')?;
        let (a, b) = (crate::stock_data::py_float(a)?, crate::stock_data::py_float(b)?);
        let (low, high) = if b < a { (b, a) } else { (a, b) };
        return Some(((low + high) / 2.0, low, high));
    }
    let value = crate::stock_data::py_float(&text)?;
    Some((value, value, value))
}

impl MiningData {
    /// Which raw materials a scanned value can be (`mining.find_signature`): the signature of one chunk times
    /// the chunks, the nearest first. Without `~` nothing is rounded: a value that hits nothing says so.
    pub fn find_signature(&self, query: &str) -> Vec<SignatureHit> {
        let Some((value, low, high)) = signature_range(query) else { return Vec::new() };
        let mut hits = Vec::new();
        let mut add = |name: &str, signature: f64, most: u32| {
            for chunks in 1..=most {
                let total = signature * f64::from(chunks);
                if low <= total && total <= high {
                    let deviation = if value != 0.0 { (total - value) / value * 100.0 } else { 0.0 };
                    let name = if name.is_empty() { "?" } else { name };
                    hits.push(SignatureHit { name: name.to_string(), chunks, total, deviation });
                }
            }
        };
        for element in self.elements.iter().filter(|e| e.signature != 0.0) {
            add(&element.name, element.signature, max_chunks(&element.rarity));
        }
        for (name, signature, most) in self.deposits() {
            add(&name, signature, most);
        }
        hits.sort_by(|a, b| a.deviation.abs().total_cmp(&b.deviation.abs()).then_with(|| a.name.cmp(&b.name)));
        hits
    }

    /// The other way (`mining.signatures_of`): per raw material whose name holds the text, the signature of
    /// one chunk and of every count up to the most, by name. A text without a letter is no name.
    pub fn signatures_of(&self, query: &str) -> Vec<(String, f64, Vec<f64>)> {
        let text = lower(crate::text::py_strip(query));
        if text.is_empty() || !text.chars().any(char::is_alphabetic) {
            return Vec::new();
        }
        let series = |signature: f64, most: u32| (1..=most).map(|n| signature * f64::from(n)).collect::<Vec<_>>();
        let mut rows: Vec<(String, f64, Vec<f64>)> = Vec::new();
        for element in self.elements.iter().filter(|e| e.signature != 0.0 && lower(&e.name).contains(&text)) {
            let row = (element.name.clone(), element.signature, series(element.signature, max_chunks(&element.rarity)));
            // A later raw material of the same name takes the place of the earlier one.
            match rows.iter_mut().find(|(name, _, _)| *name == element.name) {
                Some(old) => *old = row,
                None => rows.push(row),
            }
        }
        for (name, signature, most) in self.deposits() {
            if lower(&name).contains(&text) && !rows.iter().any(|(n, _, _)| *n == name) {
                rows.push((name, signature, series(signature, most)));
            }
        }
        rows.sort_by_cached_key(|(name, _, _)| lower(name));
        rows
    }
}

fn aliases_of(data: &Value) -> HashMap<String, String> {
    let mut result: HashMap<String, String> = HashMap::new();
    let mut by_guid: HashMap<String, String> = HashMap::new();
    if let Some(Value::Object(elements)) = data.get("elemente") {
        for (guid, element) in elements {
            let (name, material) = (text_of(element.get("name")), text_of(element.get("materialName")));
            if name.is_empty() || material.is_empty() {
                continue;
            }
            by_guid.insert(guid.clone(), norm_material(material));
            if norm_material(name) != norm_material(material) {
                result.insert(norm_material(name), norm_material(material));
            }
        }
    }
    if let Some(Value::Object(compositions)) = data.get("compositions") {
        for (_, composition) in compositions {
            let guids: BTreeSet<&str> =
                items(composition.get("parts")).iter().map(|p| text_of(p.get("elementGuid"))).collect();
            let name = text_of(composition.get("name"));
            if guids.len() != 1 || name.is_empty() {
                continue;
            }
            let Some(material) = guids.iter().next().and_then(|guid| by_guid.get(*guid)) else { continue };
            let key = norm_material(name);
            if &key != material && !by_guid.values().any(|v| *v == key) {
                result.entry(key).or_insert_with(|| material.clone());
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> MiningData {
        let text = r#"{"format": 1, "locations": [
          {"locationName": "Daymar", "system": "Stanton", "groups": [
            {"groupName": "SpaceShip_Mineables", "groupProbability": 1.0, "deposits": [
              {"compositionGuid": "c1", "relativeProbability": 3}, {"compositionGuid": "c2", "relativeProbability": 1}]},
            {"groupName": "FPS_Mineables", "deposits": [{"compositionGuid": "c3", "relativeProbability": 1}]},
            {"groupName": "Harvestables", "deposits": [{"compositionGuid": "c1", "relativeProbability": 1}]}]},
          {"locationName": "Aaron Halo", "system": "Stanton", "groups": [
            {"groupName": "SpaceShip_Mineables", "groupProbability": 0.5, "deposits": [
              {"compositionGuid": "c2", "relativeProbability": 1}]}]}],
         "compositions": {
           "c1": {"name": "Iron rock", "parts": [{"elementName": "Iron", "elementGuid": "e1", "probability": 1, "minPercent": 40, "maxPercent": 60}]},
           "c2": {"name": "Mixed", "parts": [{"elementName": "Iron", "probability": 0.5, "minPercent": 10, "maxPercent": 10},
                                              {"elementName": "Gold", "probability": 1, "minPercent": 10, "maxPercent": 10}]},
           "c3": {"parts": [{"elementName": "Hadanite", "probability": 1, "minPercent": 50, "maxPercent": 50}]}},
         "elemente": {"e1": {"name": "Iron (Ore)", "materialName": "Iron"}}}"#;
        MiningData::of(&jsonio::loads(text).unwrap())
    }

    #[test]
    fn the_richest_place_comes_first_and_every_way_of_mining_is_a_pot_of_its_own() {
        let mining = data();
        let iron = mining.ores().iter().find(|o| o.name == "Iron").unwrap();
        // Daymar by ship: Iron weighs 3/4 * 50 + 1/4 * 0.5 * 10 = 38.75 against Gold 2.5: 93.9 %.
        assert_eq!(iron.places[0].name, "Daymar");
        assert!((iron.places[0].share - 38.75 / 41.25).abs() < 1e-9, "{:?}", iron.places);
        assert_eq!(level(iron.places[0].share), 6);
        // Aaron Halo: Iron 0.5 * 0.5 * 10 = 2.5 against Gold 5: a third.
        assert!((iron.places[1].share - 1.0 / 3.0).abs() < 1e-9);
        // The hand-mined ore stands in its own pot: 100 % at Daymar.
        let hadanite = mining.ores().iter().find(|o| o.name == "Hadanite").unwrap();
        assert_eq!(hadanite.places[0].share, 1.0);
        assert!(hadanite.places[0].kinds.contains("fps"));
        assert!(mining.ores().iter().all(|o| !o.name.is_empty()));
    }

    #[test]
    fn the_ways_of_mining_gather_over_all_places_and_the_rare_ship_rocks_count_with_the_ship() {
        let text = r#"{"format": 1, "locations": [
          {"locationName": "Daymar", "groups": [
            {"groupName": "FPS_Mineables", "deposits": [{"compositionGuid": "c1"}]},
            {"groupName": "SpaceShip_Mineables_Rare", "deposits": [{"compositionGuid": "c2"}]}]},
          {"locationName": "Yela", "groups": [
            {"groupName": "GroundVehicle_Mineables", "deposits": [{"compositionGuid": "c1"}]}]}],
         "compositions": {
           "c1": {"parts": [{"elementName": "Iron", "minPercent": 10, "maxPercent": 20}]},
           "c2": {"parts": [{"elementName": "Gold", "minPercent": 10, "maxPercent": 20}]}},
         "elemente": {"e1": {"name": "Iron (Ore)", "materialName": "Iron"}}}"#;
        let mining = MiningData::of(&jsonio::loads(text).unwrap());
        let kinds = |name: &str| mining.mining_kinds(name).into_iter().collect::<Vec<_>>();
        assert_eq!(kinds("Iron"), ["fahrzeug", "fps"], "both places");
        assert_eq!(kinds("iron (ore)"), ["fahrzeug", "fps"], "by the name of the ore data too");
        assert_eq!(kinds("Gold"), ["schiff"]);
        assert!(kinds("Taranite").is_empty());
    }

    #[test]
    fn a_name_of_the_ore_data_leads_to_the_material() {
        let mining = data();
        assert_eq!(mining.material_key("Iron (Ore)"), "iron");
        assert_eq!(mining.material_key("Gold"), "gold");
    }

    #[test]
    fn every_tool_counts_for_itself_and_says_how_many_ores_it_finds_there() {
        let mining = data();
        let iron = mining.ores().iter().find(|o| o.name == "Iron").unwrap();
        let tools: Vec<(&str, usize)> =
            iron.places[0].per_tool.iter().map(|t| (t.tool.as_str(), t.ores_there)).collect();
        assert_eq!(tools, [("schiff", 2)], "Iron and Gold by ship at Daymar");
        let hadanite = mining.ores().iter().find(|o| o.name == "Hadanite").unwrap();
        assert_eq!(hadanite.places[0].per_tool[0].ores_there, 1, "the only one for the hand there");
        let daymar = mining.places().iter().find(|p| p.name == "Daymar").unwrap();
        assert_eq!(daymar.per_tool.iter().map(|t| t.tool.as_str()).collect::<Vec<_>>(), ["schiff", "fps"]);
        assert_eq!(daymar.ores.iter().map(|o| o.name.as_str()).collect::<Vec<_>>(), ["Iron", "Gold", "Hadanite"]);
        assert_eq!(mining.places().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Aaron Halo", "Daymar"]);
    }

    fn scanned() -> MiningData {
        let text = r#"{"format": 1, "locations": [], "elemente": {
            "q": {"name": "Quantanium", "scanSignature": 3170, "rarity": "legendary"},
            "i": {"name": "Iron", "scanSignature": 3000, "rarity": "common"},
            "n": {"name": "Nothing", "scanSignature": 0}}}"#;
        MiningData::of(&jsonio::loads(text).unwrap())
    }

    fn rocks(entries: &str) -> Value {
        jsonio::loads(&format!(r#"{{"format": 1, "build": "x", "steine": [{entries}]}}"#)).unwrap()
    }

    #[test]
    fn the_signatures_of_the_game_data_replace_those_of_a_raw_material_that_has_one_and_add_the_asteroid_types() {
        let mut mining = scanned();
        mining.apply_rocks(&rocks(
            r#"{"klasse": "a", "name": "Quantanium", "signatur": 3175.0},
               {"klasse": "b", "name": "Asteroid (C-Type)", "signatur": 4700.0},
               {"klasse": "c", "name": "Asteroid (C-Type)", "signatur": 4700.0},
               {"klasse": "d", "name": "Asteroid (E-Type)", "signatur": 4900.0}"#,
        ));
        let signature = |name: &str| mining.elements().iter().find(|e| e.name == name).map(|e| e.signature);
        assert_eq!(signature("Quantanium"), Some(3175.0));
        assert_eq!(signature("Iron"), Some(3000.0), "no rock of that name: as it was");
        let hits: Vec<String> = mining.find_signature("4,700").into_iter().map(|h| h.name).collect();
        assert_eq!(hits, ["Asteroid (C-Type)"]);
        assert_eq!(
            mining.find_signature("4,900").into_iter().map(|h| (h.name, h.chunks)).collect::<Vec<_>>(),
            [("Asteroid (E-Type)".to_string(), 1)]
        );
        // Several rocks of an asteroid type count as with the ores: two C-Type are 9,400, six are 28,200, seven
        // do not exist.
        let named =
            |query: &str| mining.find_signature(query).into_iter().map(|h| (h.name, h.chunks)).collect::<Vec<_>>();
        assert_eq!(named("9,400"), [("Asteroid (C-Type)".to_string(), 2)]);
        assert_eq!(named("14,100"), [("Asteroid (C-Type)".to_string(), 3)]);
        assert_eq!(named("28,200"), [("Asteroid (C-Type)".to_string(), 6)]);
        assert_eq!(named("29,400"), [("Asteroid (E-Type)".to_string(), 6)]);
        assert!(named("32,900").is_empty(), "no seventh rock");
        let rows = mining.signatures_of("aster");
        assert_eq!(rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), ["Asteroid (C-Type)", "Asteroid (E-Type)"]);
        assert_eq!(rows[0].2, [4700.0, 9400.0, 14100.0, 18800.0, 23500.0, 28200.0]);
        let values = crate::signature_scan::possible_values(&mining);
        assert!(values.contains(&4700) && values.contains(&4900) && values.contains(&3175));
        assert!(values.contains(&9400) && values.contains(&28_200) && !values.contains(&32_900), "{values:?}");
        assert!(!values.contains(&3170), "the old signature of Quantanium is gone: {values:?}");
    }

    #[test]
    fn a_rock_the_scanner_does_not_show_adds_nothing() {
        let before = crate::signature_scan::possible_values(&scanned());
        let mut mining = scanned();
        // A raw material without a signature in the file (hand mining), a deposit of the ground, a name the rocks
        // state twice with different numbers, a number that is no signature and a file of another format.
        mining.apply_rocks(&rocks(
            r#"{"klasse": "a", "name": "Nothing", "signatur": 3000.0},
               {"klasse": "b", "name": "Igneous Deposit", "signatur": 4000.0},
               {"klasse": "c", "name": "Iron", "signatur": 4000.0},
               {"klasse": "d", "name": "Iron", "signatur": 3000.0},
               {"klasse": "e", "name": "Asteroid (S-Type)", "signatur": 0.0},
               {"klasse": "f", "name": null, "signatur": 3000.0}"#,
        ));
        mining.apply_rocks(
            &jsonio::loads(r#"{"format": 2, "steine": [{"name": "Asteroid (P-Type)", "signatur": 4750}]}"#).unwrap(),
        );
        assert_eq!(crate::signature_scan::possible_values(&mining), before);
        assert_eq!(mining.elements().iter().find(|e| e.name == "Iron").map(|e| e.signature), Some(3000.0));
        assert_eq!(mining.elements().iter().find(|e| e.name == "Nothing").map(|e| e.signature), Some(0.0));
        assert_eq!(mining.deposits(), scanned().deposits(), "no ground deposit became a deposit of the scanner");
        // Values no other signature makes: a raw material without a signature and a ground deposit add none.
        let mut odd = scanned();
        odd.apply_rocks(&rocks(
            r#"{"klasse": "a", "name": "Nothing", "signatur": 4321.0},
               {"klasse": "b", "name": "Igneous Deposit", "signatur": 4323.0}"#,
        ));
        let values = crate::signature_scan::possible_values(&odd);
        assert!(!values.contains(&4321) && !values.contains(&4323), "{values:?}");
    }

    #[test]
    fn the_file_of_the_game_data_beside_the_mining_data_is_read_with_it() {
        let folder = crate::testdir::fresh("bergbau-felsen");
        let mining_file = folder.join(FILE).to_string_lossy().into_owned();
        std::fs::write(
            &mining_file,
            r#"{"format": 1, "locations": [], "elemente": {"q": {"name": "Quantanium", "scanSignature": 3170, "rarity": "legendary"}}}"#,
        )
        .unwrap();
        assert!(MiningData::load(&mining_file).deposits().iter().all(|(name, _, _)| !name.starts_with("Asteroid")));
        std::fs::write(
            folder.join(ROCK_SIGNATURES_FILE),
            r#"{"format": 1, "build": null, "steine": [{"klasse": "a", "name": "Asteroid (Q-Type)", "signatur": 4870.0}]}"#,
        )
        .unwrap();
        let mining = MiningData::load(&mining_file);
        assert_eq!(mining.deposits().iter().filter(|(name, _, _)| name == "Asteroid (Q-Type)").count(), 1);
        // A file that cannot be read leaves the data as it was.
        std::fs::write(folder.join(ROCK_SIGNATURES_FILE), "{kaputt").unwrap();
        assert!(MiningData::load(&mining_file).deposits().iter().all(|(name, _, _)| !name.starts_with("Asteroid")));
    }

    #[test]
    fn a_typed_signature_is_read_as_the_hud_writes_it() {
        assert_eq!(signature_range("17,200"), Some((17200.0, 17200.0, 17200.0)));
        assert_eq!(signature_range("17.200"), Some((17200.0, 17200.0, 17200.0)));
        let (value, low, high) = signature_range("~5,000").unwrap();
        assert_eq!(value, 5000.0);
        assert!((low - 4500.0).abs() < 1e-9 && (high - 5500.0).abs() < 1e-9);
        assert_eq!(signature_range("9,000-4,000"), Some((6500.0, 4000.0, 9000.0)));
        assert_eq!(signature_range("-5"), Some((-5.0, -5.0, -5.0)));
        for nothing in ["", "  ", "-1-2", "~", "4000-", "~1-2", "abc"] {
            assert_eq!(signature_range(nothing), None, "{nothing:?}");
        }
    }

    #[test]
    fn a_scanned_value_names_the_ore_and_the_chunks_the_nearest_first() {
        let mining = scanned();
        let hits =
            |query: &str| mining.find_signature(query).into_iter().map(|h| (h.name, h.chunks)).collect::<Vec<_>>();
        assert_eq!(hits("6340"), [("Quantanium".to_string(), 2)]);
        // A tie in the deviation goes by the name as it is written: capitals first.
        // The hand mining (3,000) and the ROC (4,000) name deposits of their own beside the ore and the salvage.
        assert_eq!(hits("3000"), [("Iron".to_string(), 1), ("fps".to_string(), 1)]);
        assert_eq!(hits("4000"), [("roc".to_string(), 1), ("salvage".to_string(), 2)]);
        assert_eq!(
            hits("12000"),
            [("Iron".to_string(), 4), ("fps".to_string(), 4), ("roc".to_string(), 3), ("salvage".to_string(), 6)]
        );
        let near = mining.find_signature("~6400");
        assert_eq!(near[0].name, "Quantanium");
        assert!((near[0].deviation - (6340.0 - 6400.0) / 6400.0 * 100.0).abs() < 1e-12);
        let rest: Vec<&str> = near[1..].iter().map(|h| h.name.as_str()).collect();
        assert_eq!(rest, ["Iron", "fps", "salvage"]);
        // Without "~" nothing is rounded, and a legendary ore comes twice at most.
        assert!(hits("8600").is_empty());
        assert!(hits("9510").is_empty(), "three Quantanium chunks do not exist");
        assert!(hits("").is_empty());
    }

    #[test]
    fn a_name_lists_the_signatures_it_can_show() {
        let mining = scanned();
        assert_eq!(mining.signatures_of("quant"), [("Quantanium".to_string(), 3170.0, vec![3170.0, 6340.0])]);
        let names: Vec<String> = mining.signatures_of("RO").into_iter().map(|(n, _, _)| n).collect();
        assert_eq!(names, ["Iron", "roc"], "the ROC has a row of its own");
        let roc = mining.signatures_of("roc");
        assert_eq!(roc.len(), 1);
        assert_eq!(roc[0].2, [4000.0, 8000.0, 12000.0, 16000.0, 20000.0, 24000.0, 28000.0], "seven chunks at most");
        assert_eq!(mining.signatures_of("fps")[0].2.len(), 10, "ten hand-mined chunks at most");
        assert_eq!(mining.signatures_of("salv").len(), 1, "the salvage has one");
        assert!(mining.signatures_of("1234").is_empty(), "digits are no name");
        assert!(mining.signatures_of("nothing").is_empty(), "no signature, no row");
    }

    #[test]
    fn the_levels_are_fixed_bands_on_the_rounded_percent() {
        assert_eq!(level(0.4949), 5);
        assert_eq!(level(0.4999), 6, "49.99 rounds to 50, the band is on the rounded percent");
        assert_eq!(level(0.5), 6);
        assert_eq!(level(0.0149), 1);
        assert_eq!(level(0.02), 2);
    }
}
