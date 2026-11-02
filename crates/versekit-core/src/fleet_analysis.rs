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

//! Fleet analysis: what the own hangar covers and what is missing.
//!
//! The base is the bundled `daten/flottenanalyse-v5.json`, taken from the
//! Fleetmanifest by Maluh (KRT, MIT, licence text in
//! `daten/flottenanalyse-LICENSE.txt`, see [`LICENSE`]): per ship the
//! gameplay loops (cargo, mining, fighters ...) with a primary, secondary
//! or planned role, for snubs the motherships, whether the ship can be
//! upgraded by CCU, and which vessels a ship brings along. The assignment of
//! ships to loops is the author's assessment, not game data.
//!
//! Every loop belongs to one of three archetypes (combat, industry and
//! supply, travel and exploration). Snubs and ground vehicles are `multi`
//! loops: they appear in every archetype, with the vehicles of that
//! purpose.
//!
//! The melt state of a ship is meltable x upgradeable by CCU; ships bought
//! in the game can be neither melted nor upgraded and get none.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::errors;
use crate::hangar;
use crate::jsonio::{self, Map, Value};
use crate::language::Lang;

/// The bundled data, trimmed by `tools/fleetmanifest_convert.py`.
const BUNDLED: &str = include_str!("../../../daten/flottenanalyse-v5.json");
/// The licence text of the data (MIT, Maluh).
pub const LICENSE: &str = include_str!("../../../daten/flottenanalyse-LICENSE.txt");

/// What a ship does in a loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Primary,
    Secondary,
    Planned,
}

impl Role {
    fn of(text: &str) -> Option<Role> {
        match text {
            "primary" => Some(Role::Primary),
            "secondary" => Some(Role::Secondary),
            "planned" => Some(Role::Planned),
            _ => None,
        }
    }
}

/// The three archetypes, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Archetype {
    Combat,
    Supply,
    Explore,
}

impl Archetype {
    pub const ALL: [Archetype; 3] = [Archetype::Combat, Archetype::Supply, Archetype::Explore];

    fn of(text: &str) -> Option<Archetype> {
        match text {
            "combat" => Some(Archetype::Combat),
            "supply" => Some(Archetype::Supply),
            "explore" => Some(Archetype::Explore),
            _ => None,
        }
    }

    /// The text key of the heading.
    pub fn key(self) -> &'static str {
        match self {
            Archetype::Combat => "s_fav_gruppe_kampf",
            Archetype::Supply => "s_fav_gruppe_versorgung",
            Archetype::Explore => "s_fav_gruppe_reisen",
        }
    }
}

/// One gameplay loop. `group` is `None` for the `multi` loops (snub, ground
/// vehicle).
#[derive(Clone, Debug)]
pub struct Loop {
    pub id: String,
    pub group: Option<Archetype>,
    de: String,
    en: String,
}

impl Loop {
    pub fn name(&self, lang: Lang) -> &str {
        match lang {
            Lang::De => &self.de,
            Lang::En => &self.en,
        }
    }
}

/// One ship of the data.
#[derive(Clone, Debug)]
pub struct Ship {
    pub name: String,
    aliases: Vec<String>,
    pub size: String,
    pub status: String,
    /// Loop index and role.
    loops: Vec<(usize, Role)>,
    pub motherships: Vec<String>,
    /// False only where RSI offers no CCU upgrade for the ship.
    pub upgradeable: bool,
    archetype: Option<Archetype>,
}

/// Where the data comes from.
#[derive(Clone, Debug, Default)]
pub struct Source {
    pub repository: String,
    pub commit: String,
    pub commit_date: String,
    pub ships_updated: String,
}

/// The loaded data with its name index.
#[derive(Debug, Default)]
pub struct Data {
    pub source: Source,
    pub loops: Vec<Loop>,
    pub ships: Vec<Ship>,
    /// Normalised parent name -> names of the vessels it brings along.
    included: Vec<(String, Vec<String>)>,
    /// Normalised name or alias -> ship; the main name wins over an alias.
    index: HashMap<String, usize>,
    /// The keys of the index, longest first.
    keys: Vec<String>,
}

/// How the Fleetmanifest compares: lower case, every run of other
/// characters becomes one space.
pub fn normalize(name: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in name.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        } else {
            gap = true;
        }
    }
    out
}

/// Manufacturer names exports put before the ship name ("Drake Cutlass
/// Black"). Without them the name would not match.
const MAKERS: [&str; 30] = [
    "roberts space industries",
    "consolidated outland",
    "musashi industrial starflight concern",
    "aegis dynamics",
    "aegis",
    "anvil aerospace",
    "anvil",
    "aopoa",
    "argo astronautics",
    "argo",
    "banu",
    "crusader industries",
    "crusader",
    "cnou",
    "drake interplanetary",
    "drake",
    "esperia",
    "gatac",
    "greycat",
    "kruger intergalactic",
    "kruger",
    "misc",
    "mirai",
    "origin jumpworks",
    "origin",
    "rsi",
    "tumbril",
    "vanduul",
    "xi an",
    "xian",
];

fn text_of(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").to_string()
}

fn strings_of(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

impl Data {
    /// Reads the data text; where it does not parse the result is empty.
    pub fn parse(source: &str) -> Data {
        let root = match jsonio::loads(source) {
            Ok(Value::Object(root)) => root,
            Ok(_) => {
                errors::record("fleet_analysis.load", &"the data is not an object");
                return Data::default();
            }
            Err(e) => {
                errors::record("fleet_analysis.load", &e);
                return Data::default();
            }
        };
        let get = |key: &str| root.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        let mut data = Data::default();
        if let Some(Value::Object(src)) = get("source") {
            let field = |key: &str| text_of(src.iter().find(|(k, _)| k == key).map(|(_, v)| v));
            data.source = Source {
                repository: field("repository"),
                commit: field("commit"),
                commit_date: field("commit_date"),
                ships_updated: field("ships_updated"),
            };
        }
        if let Some(Value::Array(items)) = get("loops") {
            for item in items.iter().filter_map(Value::as_object) {
                let field = |key: &str| text_of(item.iter().find(|(k, _)| k == key).map(|(_, v)| v));
                // Only the word "multi" means all archetypes; a group the
                // program does not know would show up everywhere unnoticed.
                let group = match field("group").as_str() {
                    "multi" => None,
                    other => match Archetype::of(other) {
                        Some(a) => Some(a),
                        None => {
                            errors::record("fleet_analysis.load", &format!("unknown group of loop {}", field("id")));
                            continue;
                        }
                    },
                };
                data.loops.push(Loop { id: field("id"), group, de: field("de"), en: field("en") });
            }
        }
        let loop_index: HashMap<&str, usize> = data.loops.iter().enumerate().map(|(i, l)| (l.id.as_str(), i)).collect();
        if let Some(Value::Array(items)) = get("ships") {
            for item in items.iter().filter_map(Value::as_object) {
                let field = |key: &str| item.iter().find(|(k, _)| k == key).map(|(_, v)| v);
                let mut loops = Vec::new();
                if let Some(Value::Object(roles)) = field("loops") {
                    for (id, role) in roles {
                        // A role of a loop that does not exist is dropped.
                        if let (Some(&at), Some(role)) = (loop_index.get(id.as_str()), role.as_str().and_then(Role::of))
                        {
                            loops.push((at, role));
                        }
                    }
                }
                data.ships.push(Ship {
                    name: text_of(field("name")),
                    aliases: strings_of(field("aliases")),
                    size: text_of(field("size")),
                    status: text_of(field("status")),
                    loops,
                    motherships: strings_of(field("motherships")),
                    upgradeable: !matches!(field("upgradeable"), Some(Value::Bool(false))),
                    archetype: Archetype::of(&text_of(field("archetype"))),
                });
            }
        }
        if let Some(Value::Object(parents)) = get("included") {
            for (parent, list) in parents {
                data.included.push((normalize(parent), strings_of(Some(list))));
            }
        }
        data.build_index();
        data
    }

    fn build_index(&mut self) {
        for (i, ship) in self.ships.iter().enumerate() {
            self.index.insert(normalize(&ship.name), i);
        }
        for (i, ship) in self.ships.iter().enumerate() {
            for alias in &ship.aliases {
                self.index.entry(normalize(alias)).or_insert(i);
            }
        }
        self.index.remove("");
        self.keys = self.index.keys().cloned().collect();
        self.keys.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    }

    /// The ship of the data for a hangar name, or `None`:
    /// 1. name or alias exactly (case and punctuation do not count),
    /// 2. a data name as the beginning: `Cutlass Black Best In Show Edition`
    ///    finds `Cutlass Black`,
    /// 3. the same without a manufacturer put before the name.
    pub fn find(&self, name: &str, manufacturer: &str) -> Option<&Ship> {
        self.find_at(name, manufacturer).map(|i| &self.ships[i])
    }

    fn find_at(&self, name: &str, manufacturer: &str) -> Option<usize> {
        let candidates = candidates(name, manufacturer);
        if let Some(i) = candidates.iter().find_map(|c| self.index.get(c)) {
            return Some(*i);
        }
        candidates.iter().find_map(|c| {
            self.keys
                .iter()
                .find(|key| c.strip_prefix(key.as_str()).is_some_and(|rest| rest.starts_with(' ')))
                .and_then(|key| self.index.get(key).copied())
        })
    }

    fn by_name(&self, name: &str) -> Option<usize> {
        self.find_at(name, "")
    }

    /// The archetypes a ship counts in: the one the data names, else the
    /// groups of its primary and planned roles, else those of its secondary
    /// roles. The `multi` loops (snub, ground vehicle) do not decide.
    pub fn archetypes_of(&self, ship: &Ship) -> Vec<Archetype> {
        if let Some(a) = ship.archetype {
            return vec![a];
        }
        let groups = |wanted: &[Role]| -> Vec<Archetype> {
            Archetype::ALL
                .into_iter()
                .filter(|a| ship.loops.iter().any(|(l, r)| wanted.contains(r) && self.loops[*l].group == Some(*a)))
                .collect()
        };
        let core = groups(&[Role::Primary, Role::Planned]);
        if core.is_empty() { groups(&[Role::Secondary]) } else { core }
    }

    /// The display slots in archetype order: every loop once, the `multi`
    /// loops once per archetype.
    pub fn slots(&self) -> Vec<Slot> {
        let mut slots = Vec::new();
        for group in Archetype::ALL {
            for (i, l) in self.loops.iter().enumerate() {
                if l.group == Some(group) {
                    slots.push(Slot { loop_at: i, group, multi: false });
                }
            }
            for (i, l) in self.loops.iter().enumerate() {
                if l.group.is_none() {
                    slots.push(Slot { loop_at: i, group, multi: true });
                }
            }
        }
        slots
    }

    fn role_in(&self, ship: usize, slot: &Slot) -> Option<Role> {
        let ship = &self.ships[ship];
        let role = ship.loops.iter().find(|(l, _)| *l == slot.loop_at).map(|(_, r)| *r)?;
        (!slot.multi || self.archetypes_of(ship).contains(&slot.group)).then_some(role)
    }
}

/// The bundled data.
pub fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| Data::parse(BUNDLED))
}

/// The normalised names to try: the name, and without a manufacturer
/// in front of it.
fn candidates(name: &str, manufacturer: &str) -> Vec<String> {
    let first = normalize(name);
    let maker = normalize(manufacturer);
    let mut out = vec![first.clone()];
    let makers = std::iter::once(maker.as_str()).filter(|m| !m.is_empty()).chain(MAKERS.iter().copied());
    for m in makers {
        if let Some(rest) = first.strip_prefix(m).and_then(|r| r.strip_prefix(' ')) {
            out.push(rest.to_string());
            break;
        }
    }
    out
}

/// One place on the page: a loop inside an archetype.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    loop_at: usize,
    pub group: Archetype,
    /// A snub or ground vehicle place: counts only the vehicles of its
    /// archetype and is never a gap.
    pub multi: bool,
}

impl Slot {
    pub fn lop<'a>(&self, data: &'a Data) -> &'a Loop {
        &data.loops[self.loop_at]
    }

    /// A key that stays the same for the place (to remember folded ones).
    pub fn key(&self, data: &Data) -> String {
        let id = &self.lop(data).id;
        if self.multi { format!("{id}--{}", self.group.key()) } else { id.clone() }
    }
}

/// A ship in a list: its name in the hangar, or in the data when it
/// only comes along with another ship.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub name: String,
    pub included: bool,
}

/// The ships of one place by role.
#[derive(Clone, Debug)]
pub struct SlotResult {
    pub slot: Slot,
    pub primary: Vec<Listed>,
    pub secondary: Vec<Listed>,
    pub planned: Vec<Listed>,
}

impl SlotResult {
    /// No ship at all.
    pub fn empty(&self) -> bool {
        self.primary.is_empty() && self.secondary.is_empty() && self.planned.is_empty()
    }

    /// No ship in a primary or planned role. Not for the `multi` places.
    pub fn gap(&self) -> bool {
        !self.slot.multi && self.primary.is_empty() && self.planned.is_empty()
    }
}

/// A vessel that comes along with a ship in the hangar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Included {
    pub name: String,
    pub parents: Vec<String>,
}

/// The melt state: meltable x upgradeable by CCU.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeltState {
    /// Melt and CCU.
    Both,
    /// Melt, but no CCU.
    MeltOnly,
    /// Not meltable, CCU only.
    Upgrade,
    /// Neither.
    Locked,
}

impl MeltState {
    /// The text key of the caption.
    pub fn key(self) -> &'static str {
        match self {
            MeltState::Both => "s_fa_m_beides",
            MeltState::MeltOnly => "s_fa_m_nur_s",
            MeltState::Upgrade => "s_fa_m_nur_c",
            MeltState::Locked => "s_fa_m_keins",
        }
    }
}

/// `(meltable, upgradeable)` -> state; `None` where it is not known.
pub fn state_for(meltable: Option<bool>, upgradeable: bool) -> Option<MeltState> {
    Some(match (meltable?, upgradeable) {
        (true, true) => MeltState::Both,
        (true, false) => MeltState::MeltOnly,
        (false, true) => MeltState::Upgrade,
        (false, false) => MeltState::Locked,
    })
}

/// Can the ship be upgraded by CCU? Without a match in the data: yes.
pub fn upgradeable(entry: &Map) -> bool {
    data()
        .find(hangar::field_text(entry, "name"), hangar::field_text(entry, "hersteller"))
        .is_none_or(|s| s.upgradeable)
}

/// Meltable: the choice on the ship card wins over the import; `None` when
/// unknown.
pub fn meltable(entry: &Map) -> Option<bool> {
    match hangar::melt_choice(entry) {
        hangar::CHOICE_YES => Some(true),
        hangar::CHOICE_NO => Some(false),
        _ => hangar::melt_imported(entry),
    }
}

/// The melt state of a hangar ship; `None` when it is unknown or does not apply.
pub fn melt_state(entry: &Map) -> Option<MeltState> {
    if !hangar::melt_applies(entry) {
        return None;
    }
    state_for(meltable(entry), upgradeable(entry))
}

/// How many ships are in which melt state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeltCount {
    pub both: usize,
    pub melt_only: usize,
    pub upgrade: usize,
    pub locked: usize,
    /// Ships where a state applies but is not known yet.
    pub unknown: usize,
}

/// The result over the hangar entries.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    /// The places in display order.
    pub slots: Vec<SlotResult>,
    /// Hangar names that matched nothing, sorted.
    pub unknown: Vec<String>,
    /// Snubs whose mothership is not in the hangar, with the motherships.
    pub orphan_snubs: Vec<(String, Vec<String>)>,
    /// Vessels that come along with another ship.
    pub included: Vec<Included>,
    pub melt: MeltCount,
    /// Hangar ships that matched.
    pub matched: usize,
}

impl Analysis {
    /// The places of one archetype.
    pub fn of(&self, group: Archetype) -> impl Iterator<Item = &SlotResult> {
        self.slots.iter().filter(move |s| s.slot.group == group)
    }
}

/// A ship of the data as owned: with the hangar names that match it, or
/// only included by other ships.
struct Owned {
    ship: usize,
    names: Vec<String>,
    /// A hangar entry that is no vehicle of the bundled catalog (those the
    /// hangar adds itself for a ship that brings them along).
    own: bool,
    /// The ships that bring it along.
    included_by: Vec<usize>,
}

impl Owned {
    fn new(ship: usize) -> Owned {
        Owned { ship, names: Vec::new(), own: false, included_by: Vec::new() }
    }

    /// It comes along with another ship and is not owned on its own.
    fn only_included(&self) -> bool {
        !self.own
    }
}

fn by_lower(a: &Listed, b: &Listed) -> std::cmp::Ordering {
    a.name.to_lowercase().cmp(&b.name.to_lowercase())
}

/// The analysis over the hangar ships.
///
/// A ship that stands twice in the hangar counts once per loop. When two
/// different hangar names match the same record (`ATLS GEO` and `ATLS
/// IKTI` -> `ATLS`) both names are listed. Vessels a ship brings along
/// count once each, marked as included; they do not stack.
pub fn analyse(entries: &[&Map]) -> Analysis {
    analyse_with(data(), entries)
}

/// [`analyse`] on given data.
pub fn analyse_with(data: &Data, entries: &[&Map]) -> Analysis {
    let mut result = Analysis::default();
    let mut owned: Vec<Owned> = Vec::new();
    let mut at: HashMap<usize, usize> = HashMap::new();
    // (entry, ship) of every hangar ship that matched
    let mut matched: Vec<(&Map, usize)> = Vec::new();
    for entry in entries {
        let name = hangar::field_text(entry, "name");
        if name.is_empty() {
            continue;
        }
        match data.find_at(name, hangar::field_text(entry, "hersteller")) {
            None => result.unknown.push(name.to_string()),
            Some(ship) => matched.push((entry, ship)),
        }
    }
    result.matched = matched.len();
    result.unknown.sort_by_key(|n| n.to_lowercase());
    let mut seen = std::collections::HashSet::new();
    result.unknown.retain(|n| seen.insert(n.clone()));
    for (entry, ship) in &matched {
        let slot = *at.entry(*ship).or_insert_with(|| {
            owned.push(Owned::new(*ship));
            owned.len() - 1
        });
        let name = hangar::field_text(entry, "name");
        if !owned[slot].names.iter().any(|n| n == name) {
            owned[slot].names.push(name.to_string());
        }
        owned[slot].own |= hangar::field_text(entry, "herkunft") != hangar::CATALOG;
    }

    // What the owned ships bring along: it stays with the own copy if there
    // is one, else it becomes a record of its own.
    for (entry, parent) in &matched {
        let name = hangar::field_text(entry, "name");
        let maker = hangar::field_text(entry, "hersteller");
        let parent_name = normalize(&data.ships[*parent].name);
        let list = candidates(name, maker)
            .into_iter()
            .chain(std::iter::once(parent_name))
            .find_map(|key| data.included.iter().find(|(k, _)| *k == key).map(|(_, l)| l));
        for vessel in list.into_iter().flatten() {
            let Some(vessel) = data.by_name(vessel).filter(|v| v != parent) else { continue };
            let slot = *at.entry(vessel).or_insert_with(|| {
                owned.push(Owned::new(vessel));
                owned.len() - 1
            });
            if !owned[slot].included_by.contains(parent) {
                owned[slot].included_by.push(*parent);
            }
        }
    }

    for slot in data.slots() {
        let mut found = SlotResult { slot, primary: Vec::new(), secondary: Vec::new(), planned: Vec::new() };
        for o in &owned {
            let Some(role) = data.role_in(o.ship, &slot) else { continue };
            let list = match role {
                Role::Primary => &mut found.primary,
                Role::Secondary => &mut found.secondary,
                Role::Planned => &mut found.planned,
            };
            if o.only_included() {
                list.push(Listed { name: data.ships[o.ship].name.clone(), included: true });
            } else {
                list.extend(o.names.iter().map(|n| Listed { name: n.clone(), included: false }));
            }
        }
        for list in [&mut found.primary, &mut found.secondary, &mut found.planned] {
            list.sort_by(by_lower);
        }
        result.slots.push(found);
    }

    // A snub whose mothership is not here; an included one or one a ship
    // here brings along has its mothership by definition.
    let have: Vec<&str> = owned.iter().map(|o| data.ships[o.ship].name.as_str()).collect();
    for o in &owned {
        let mothers = &data.ships[o.ship].motherships;
        if mothers.is_empty() || !o.included_by.is_empty() || mothers.iter().any(|m| have.contains(&m.as_str())) {
            continue;
        }
        result.orphan_snubs.extend(o.names.iter().map(|n| (n.clone(), mothers.clone())));
    }
    result.orphan_snubs.sort_by_key(|(n, _)| n.to_lowercase());

    for o in owned.iter().filter(|o| !o.included_by.is_empty()) {
        let parents = o.included_by.iter().map(|p| data.ships[*p].name.clone()).collect();
        result.included.push(Included { name: data.ships[o.ship].name.clone(), parents });
    }
    result.included.sort_by_key(|i| i.name.to_lowercase());

    for entry in entries {
        if !hangar::melt_applies(entry) {
            continue;
        }
        let melt = &mut result.melt;
        match melt_state(entry) {
            None => melt.unknown += 1,
            Some(MeltState::Both) => melt.both += 1,
            Some(MeltState::MeltOnly) => melt.melt_only += 1,
            Some(MeltState::Upgrade) => melt.upgrade += 1,
            Some(MeltState::Locked) => melt.locked += 1,
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> Map {
        match jsonio::loads(text).expect("JSON") {
            Value::Object(m) => m,
            _ => panic!("object"),
        }
    }

    fn pledge(name: &str) -> Map {
        entry(&format!(r#"{{"name": "{name}", "herkunft": "pledge"}}"#))
    }

    fn run(entries: &[Map]) -> Analysis {
        let refs: Vec<&Map> = entries.iter().collect();
        analyse(&refs)
    }

    fn place<'a>(a: &'a Analysis, id: &str, group: Archetype) -> &'a SlotResult {
        a.slots.iter().find(|s| s.slot.lop(data()).id == id && s.slot.group == group).expect("place")
    }

    fn names(list: &[Listed]) -> Vec<&str> {
        list.iter().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn the_bundled_data_is_the_new_state() {
        let d = data();
        assert_eq!(d.ships.len(), 243);
        assert_eq!(d.loops.len(), 22);
        assert_eq!(d.loops.iter().filter(|l| l.group.is_none()).count(), 2);
        assert_eq!(d.included.len(), 15);
        assert_eq!(d.source.commit.len(), 40);
        // three archetypes x (loops of the group + the two multi loops)
        assert_eq!(d.slots().len(), 20 + 3 * 2);
        assert!(LICENSE.contains("MIT License") && LICENSE.contains("MaluhTV"));
    }

    #[test]
    fn every_text_key_exists_in_both_languages() {
        let keys = Archetype::ALL
            .iter()
            .map(|a| a.key())
            .chain([MeltState::Both, MeltState::MeltOnly, MeltState::Upgrade, MeltState::Locked].map(MeltState::key));
        for key in keys {
            for lang in Lang::ALL {
                assert!(crate::language::text_in(lang, key).is_some(), "{key} {lang:?}");
            }
        }
        for lang in Lang::ALL {
            for l in &data().loops {
                assert!(!l.name(lang).is_empty(), "{} {lang:?}", l.id);
            }
        }
    }

    #[test]
    fn every_role_names_a_loop_and_every_mothership_a_ship() {
        let d = data();
        for ship in &d.ships {
            assert!(!ship.loops.is_empty(), "{} has no role", ship.name);
            for mother in &ship.motherships {
                assert!(d.ships.iter().any(|s| &s.name == mother), "{} -> {mother}", ship.name);
            }
        }
        for vessels in d.included.iter().flat_map(|(_, v)| v) {
            assert!(d.find(vessels, "").is_some(), "included {vessels}");
        }
    }

    #[test]
    fn names_match_exactly_by_alias_by_start_and_without_maker() {
        let d = data();
        let name = |n: &str, m: &str| d.find(n, m).map(|s| s.name.as_str());
        assert_eq!(name("Cutlass Black", ""), Some("Cutlass Black"));
        assert_eq!(name("  cutlass-BLACK ", ""), Some("Cutlass Black"));
        assert_eq!(name("600i", ""), Some("600i Explorer"));
        assert_eq!(name("Cutlass Black Best In Show Edition", ""), Some("Cutlass Black"));
        assert_eq!(name("Drake Cutlass Black", ""), Some("Cutlass Black"));
        assert_eq!(name("Cutlass Black", "Drake Interplanetary"), Some("Cutlass Black"));
        assert_eq!(name("Mercury Star Runner", ""), Some("Mercury Star Runner"));
        assert_eq!(name("Totally Unknown Ship", ""), None);
        // a start in the middle of a word does not count
        assert_eq!(name("Prospectors", ""), None);
        assert_eq!(name("", ""), None);
    }

    #[test]
    fn a_main_name_wins_over_an_alias_of_another_ship() {
        let d = Data::parse(
            r#"{"loops": [{"id": "a", "group": "combat", "de": "A", "en": "A"}], "ships": [
            {"name": "One", "aliases": ["Two"], "loops": {"a": "primary"}},
            {"name": "Two", "loops": {"a": "primary"}}]}"#,
        );
        assert_eq!(d.find("Two", "").map(|s| s.name.as_str()), Some("Two"));
        assert_eq!(d.find("Two Edition", "").map(|s| s.name.as_str()), Some("Two"));
    }

    #[test]
    fn the_longest_data_name_wins_a_start() {
        let d = Data::parse(
            r#"{"loops": [{"id": "a", "group": "combat", "de": "A", "en": "A"}], "ships": [
            {"name": "Cyclone", "loops": {"a": "primary"}},
            {"name": "Cyclone AA", "loops": {"a": "primary"}}]}"#,
        );
        assert_eq!(d.find("Cyclone AA Special", "").map(|s| s.name.as_str()), Some("Cyclone AA"));
        assert_eq!(d.find("Cyclone Special", "").map(|s| s.name.as_str()), Some("Cyclone"));
    }

    #[test]
    fn coverage_gaps_and_unknown_names() {
        let a = run(&[pledge("Prospector"), pledge("Cutlass Black"), pledge("Mystery Boat")]);
        let mining = place(&a, "mining", Archetype::Supply);
        assert_eq!(names(&mining.primary), ["Prospector"]);
        assert!(!mining.gap() && !mining.empty());
        // secondary only: still a gap, but not empty
        let fighter = place(&a, "fighter", Archetype::Combat);
        assert_eq!(names(&fighter.secondary), ["Cutlass Black"]);
        assert!(fighter.gap() && !fighter.empty());
        // nothing at all
        let racing = place(&a, "racing", Archetype::Explore);
        assert!(racing.gap() && racing.empty());
        assert_eq!(a.unknown, ["Mystery Boat"]);
        assert_eq!(a.matched, 2);
    }

    #[test]
    fn a_planned_role_is_no_gap() {
        // The G12 is a planned ground vehicle of the explore archetype.
        let a = run(&[pledge("G12")]);
        let ground = place(&a, "ground", Archetype::Explore);
        assert_eq!(names(&ground.planned), ["G12"]);
        assert!(ground.slot.multi && !ground.gap());
        assert!(place(&a, "ground", Archetype::Combat).empty());
        // An empty snub or ground vehicle place is never a gap.
        assert!(!place(&a, "ground", Archetype::Combat).gap());
        assert!(!place(&a, "snub", Archetype::Supply).gap());
    }

    #[test]
    fn the_same_ship_twice_counts_once_and_two_names_both_show() {
        let a = run(&[pledge("Prospector"), pledge("Prospector")]);
        assert_eq!(names(&place(&a, "mining", Archetype::Supply).primary), ["Prospector"]);
        let b = run(&[pledge("Prospector"), pledge("Prospector Alliance")]);
        assert_eq!(names(&place(&b, "mining", Archetype::Supply).primary), ["Prospector", "Prospector Alliance"]);
    }

    #[test]
    fn snubs_and_ground_vehicles_count_in_their_own_archetype_only() {
        let a = run(&[pledge("Ballista"), pledge("Ursa Rover")]);
        assert_eq!(names(&place(&a, "ground", Archetype::Combat).primary), ["Ballista"]);
        // no archetype named: the group of the secondary role decides (exploration)
        assert_eq!(names(&place(&a, "ground", Archetype::Explore).primary), ["Ursa Rover"]);
        assert!(place(&a, "ground", Archetype::Supply).empty());
    }

    #[test]
    fn what_a_ship_brings_along_counts_once_and_is_marked() {
        let a = run(&[pledge("Carrack")]);
        let snub = place(&a, "snub", Archetype::Explore);
        assert_eq!(snub.primary, [Listed { name: "C8 Pisces".into(), included: true }]);
        let ground = place(&a, "ground", Archetype::Explore);
        assert_eq!(ground.primary, [Listed { name: "Ursa".into(), included: true }]);
        assert_eq!(a.included.len(), 2);
        assert_eq!(
            a.included[0],
            Included { name: "C8 Pisces".into(), parents: vec!["Carrack Expedition w/C8X".into()] }
        );
        // a vessel that comes along has its mothership
        assert!(a.orphan_snubs.is_empty());
        // the parent itself is no included vessel
        let cargo = place(&a, "cargo", Archetype::Supply);
        assert!(cargo.secondary.iter().any(|l| l.name == "Carrack" && !l.included));
    }

    #[test]
    fn an_own_copy_of_an_included_vessel_is_not_counted_twice() {
        let a = run(&[pledge("Carrack"), pledge("C8 Pisces")]);
        let snub = place(&a, "snub", Archetype::Explore);
        assert_eq!(snub.primary, [Listed { name: "C8 Pisces".into(), included: false }]);
        assert!(a.orphan_snubs.is_empty());
    }

    #[test]
    fn a_vehicle_the_hangar_added_for_a_package_counts_as_included() {
        let catalog = entry(r#"{"name": "Ursa Rover", "herkunft": "katalog", "paket": "Carrack"}"#);
        let a = run(&[pledge("Carrack"), catalog]);
        let ground = place(&a, "ground", Archetype::Explore);
        assert_eq!(ground.primary, [Listed { name: "Ursa".into(), included: true }]);
        assert_eq!(a.included.iter().filter(|i| i.name == "Ursa").count(), 1);
        // The same vehicle bought on its own is an own ship.
        let own = run(&[pledge("Carrack"), pledge("Ursa Rover")]);
        assert_eq!(names(&place(&own, "ground", Archetype::Explore).primary), ["Ursa Rover"]);
    }

    #[test]
    fn every_snub_and_ground_vehicle_counts_in_some_archetype() {
        let d = data();
        for ship in &d.ships {
            let vehicle = ship.loops.iter().any(|(l, _)| d.loops[*l].group.is_none());
            if vehicle {
                assert!(!d.archetypes_of(ship).is_empty(), "{} would vanish from the page", ship.name);
            }
        }
    }

    #[test]
    fn a_loop_with_an_unknown_group_is_left_out() {
        let d = Data::parse(
            r#"{"loops": [{"id": "a", "group": "combat", "de": "A", "en": "A"},
                           {"id": "b", "group": "cmobat", "de": "B", "en": "B"},
                           {"id": "c", "group": "multi", "de": "C", "en": "C"}],
                "ships": [{"name": "One", "loops": {"a": "primary", "b": "primary"}}]}"#,
        );
        assert_eq!(d.loops.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(d.ships[0].loops.len(), 1);
    }

    #[test]
    fn a_maker_of_the_hangar_entry_is_taken_off_even_when_the_list_does_not_know_it() {
        let d = data();
        assert_eq!(d.find("Zeta Cutlass Black", "Zeta").map(|s| s.name.as_str()), Some("Cutlass Black"));
        assert_eq!(d.find("Zeta Cutlass Black", "").map(|s| s.name.as_str()), None);
    }

    #[test]
    fn an_included_snub_is_no_orphan_even_if_the_parent_is_not_its_mothership() {
        let d = Data::parse(
            r#"{"loops": [{"id": "a", "group": "combat", "de": "A", "en": "A"}], "ships": [
            {"name": "Mother", "loops": {"a": "primary"}},
            {"name": "Other", "loops": {"a": "primary"}},
            {"name": "Pod", "motherships": ["Other"], "loops": {"a": "primary"}}],
            "included": {"Mother": ["Pod"]}}"#,
        );
        // The pod is also owned on its own; the mother brings it along as well.
        let entries = [pledge("Mother"), pledge("Pod")];
        let refs: Vec<&Map> = entries.iter().collect();
        let a = analyse_with(&d, &refs);
        assert_eq!(a.included.len(), 1);
        assert!(a.orphan_snubs.is_empty(), "{:?}", a.orphan_snubs);
        let alone = [pledge("Pod")];
        let refs: Vec<&Map> = alone.iter().collect();
        assert_eq!(analyse_with(&d, &refs).orphan_snubs.len(), 1);
    }

    #[test]
    fn a_snub_without_its_mothership_is_reported() {
        let a = run(&[pledge("Fury")]);
        assert_eq!(a.orphan_snubs, [("Fury".to_string(), vec!["Starlancer TAC".to_string()])]);
        let with = run(&[pledge("Fury"), pledge("Starlancer TAC")]);
        assert!(with.orphan_snubs.is_empty());
        // an alias of the mothership is enough
        // The rescue Pisces belongs to the Carrack, which is only an alias of the data name.
        let rescue = run(&[pledge("C8R Pisces Rescue"), pledge("Carrack")]);
        assert!(rescue.orphan_snubs.is_empty());
        let alone = run(&[pledge("C8R Pisces Rescue")]);
        assert_eq!(alone.orphan_snubs.len(), 1);
    }

    #[test]
    fn melt_state_is_meltable_times_upgradeable() {
        assert_eq!(state_for(Some(true), true), Some(MeltState::Both));
        assert_eq!(state_for(Some(true), false), Some(MeltState::MeltOnly));
        assert_eq!(state_for(Some(false), true), Some(MeltState::Upgrade));
        assert_eq!(state_for(Some(false), false), Some(MeltState::Locked));
        assert_eq!(state_for(None, true), None);
    }

    #[test]
    fn the_melt_choice_wins_over_the_import_and_ingame_ships_have_none() {
        let imported = entry(r#"{"name": "Cutlass Black", "herkunft": "pledge", "schmelzbar": false}"#);
        assert_eq!(melt_state(&imported), Some(MeltState::Upgrade));
        let chosen =
            entry(r#"{"name": "Cutlass Black", "herkunft": "pledge", "schmelzbar": false, "schmelz_wahl": "ja"}"#);
        assert_eq!(melt_state(&chosen), Some(MeltState::Both));
        let nothing = pledge("Cutlass Black");
        assert_eq!(melt_state(&nothing), None);
        let ingame = entry(r#"{"name": "Cutlass Black", "herkunft": "ingame", "schmelzbar": true}"#);
        assert_eq!(melt_state(&ingame), None);
        // no CCU for the F8C Lightning Civilian
        let f8c = entry(r#"{"name": "F8C Lightning Civilian", "herkunft": "pledge", "schmelzbar": true}"#);
        assert_eq!(melt_state(&f8c), Some(MeltState::MeltOnly));
        // a name the data does not know can be upgraded
        let unknown = entry(r#"{"name": "Mystery Boat", "herkunft": "pledge", "schmelzbar": false}"#);
        assert_eq!(melt_state(&unknown), Some(MeltState::Upgrade));
    }

    #[test]
    fn the_melt_numbers_add_up_over_all_entries() {
        let entries = [
            entry(r#"{"name": "Cutlass Black", "herkunft": "pledge", "schmelzbar": true}"#),
            entry(r#"{"name": "Prospector", "herkunft": "pledge", "schmelzbar": false}"#),
            entry(r#"{"name": "F8C Lightning Civilian", "herkunft": "pledge", "schmelzbar": false}"#),
            entry(r#"{"name": "Mystery Boat", "schmelzbar": true}"#),
            pledge("Fury"),
            entry(r#"{"name": "Hammerhead", "herkunft": "ingame"}"#),
        ];
        let a = run(&entries);
        assert_eq!(a.melt, MeltCount { both: 2, melt_only: 0, upgrade: 1, locked: 1, unknown: 1 },);
    }

    #[test]
    fn broken_data_gives_an_empty_analysis_and_no_panic() {
        let d = Data::parse("not json");
        assert!(d.ships.is_empty() && d.slots().is_empty());
        let entries = [pledge("Prospector")];
        let refs: Vec<&Map> = entries.iter().collect();
        let a = analyse_with(&d, &refs);
        assert!(a.slots.is_empty());
        assert_eq!(a.unknown, ["Prospector"]);
    }
}
