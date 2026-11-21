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

//! Every contract the stored game data know, one per title key, for the contract finder: the terms and the
//! reputation of the reputation table (`auftragsruf.json`), the blueprints of the catalog (`missionen`) and the title
//! the game shows (`global.ini` that counts). A value the files do not state stays `None`; nothing is filled in.
//!
//! [`arrange`] filters and sorts the contracts as a [`View`] says; [`Choices`] are the values the filters offer.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::SystemTime;

use versekit_core::injection::reputation::{self, key};
use versekit_core::injection::terms::GIVER;
use versekit_core::jsonio::{self, Map, Value};
use versekit_core::paths::Paths;

/// A reputation a contract gives or takes on success: whom, of which kind, and the amount, `low` to `high` where the
/// contracts of the title differ. `unknown` marks a gain whose amount the game data do not state (`low` and `high`
/// are zero then): the contract gives that reputation, how much is not known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gain {
    pub who: String,
    pub kind: String,
    pub low: i64,
    pub high: i64,
    pub unknown: bool,
}

/// A rank of a faction and the reputation it starts at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rank {
    pub name: String,
    pub reputation: i64,
}

/// One contract, by the key of its title.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contract {
    /// The text key of the title, without `@`, in lower case.
    pub key: String,
    /// The title as the game shows it, placeholders the game fills in as `[NAME]`; `None` without a language file.
    pub title: Option<String>,
    /// The faction that gives the contract.
    pub giver: Option<String>,
    /// The reputation it gives; empty where the table names none.
    pub gains: Vec<Gain>,
    /// The minutes until it can be taken again, `low` to `high`; `None` where it has no personal cooldown or none is
    /// known.
    pub cooldown: Option<(i64, i64)>,
    /// How far that cooldown varies either way, in minutes.
    pub cooldown_spread: Option<i64>,
    /// Whether it can be shared in a party.
    pub shareable: Option<bool>,
    /// Where it plays, as the table writes it (`Stanton (Crusader, microTech), Pyro`).
    pub region: Option<String>,
    /// The star systems of [`Contract::region`].
    pub systems: Vec<String>,
    /// The lowest rank that gets it.
    pub rank: Option<Rank>,
    /// The highest rank that still gets it.
    pub rank_max: Option<Rank>,
    /// The blueprints it can award, by name; `None` without a catalog.
    pub blueprints: Option<Vec<String>>,
}

impl Contract {
    /// The largest reputation it gives any party, `None` without a gain whose amount is known.
    pub fn best_gain(&self) -> Option<i64> {
        self.gains.iter().filter(|gain| !gain.unknown).map(|gain| gain.high).max()
    }
}

/// The contracts of the data folder, with what the files were when they were read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Finder {
    pub contracts: Vec<Contract>,
    /// The game build of the reputation table, empty without one.
    pub version: String,
}

/// When the files the finder reads were written and how long they are: reputation table, catalog, language file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamps([Option<(SystemTime, u64)>; 3]);

impl Stamps {
    /// The stamps of the files of `paths`.
    pub fn of(paths: &Paths) -> Stamps {
        let written = |path: Option<String>| {
            let meta = std::fs::metadata(path?).ok()?;
            Some((meta.modified().ok()?, meta.len()))
        };
        Stamps([
            written(Some(paths.app_file(reputation::CACHE_FILE))),
            written(Some(versekit_core::catalog_cache::current_file(paths))),
            written(versekit_core::game_ini::ini_file(paths)),
        ])
    }
}

/// The contracts of the data folder of `paths` and the game folder it names.
pub fn load(paths: &Paths) -> Finder {
    let table = reputation::load(paths);
    let catalog = jsonio::read_json(&versekit_core::catalog_cache::current_file(paths)).ok();
    let missions = catalog.as_ref().map(versekit_core::mission_log::missions_of);
    let wanted = title_keys(&table, missions.as_ref());
    let titles = versekit_core::game_ini::ini_file(paths)
        .and_then(|path| std::fs::read(path).ok())
        .map(|bytes| titles_of(&versekit_core::text::decode_ignore(&bytes), &wanted))
        .unwrap_or_default();
    Finder { contracts: contracts(&table, missions.as_ref(), &titles), version: table.version.clone() }
}

/// The keys of every title the table or the catalog names, without `@`, in lower case.
fn title_keys(table: &reputation::Table, missions: Option<&HashMap<String, Vec<String>>>) -> HashSet<String> {
    let mut keys: HashSet<String> = table.terms.iter().chain(&table.contracts).map(|(k, _)| key(k)).collect();
    keys.extend(missions.into_iter().flat_map(|m| m.keys()).map(|k| key(k)));
    keys.retain(|k| !k.is_empty());
    keys
}

/// Title key (lower case, without `@` and without the variant `,P`) → the text a `global.ini` gives it, for the keys
/// of `wanted`: the marks of VerseKit and other tools taken out, each `~mission(Name)` as `[NAME]`. The first line of
/// a key counts.
pub fn titles_of(ini: &str, wanted: &HashSet<String>) -> HashMap<String, String> {
    let mut found = HashMap::new();
    for line in ini.lines() {
        let Some((raw, text)) = line.split_once('=') else { continue };
        let raw = raw.trim_start_matches('\u{feff}');
        let name = raw.split_once(',').map_or(raw, |(name, _)| name);
        let name = key(name.trim());
        if !wanted.contains(&name) || found.contains_key(&name) {
            continue;
        }
        let shown = placeholders(&versekit_core::mission_log::clean(text));
        if !shown.is_empty() {
            found.insert(name, shown);
        }
    }
    found
}

/// `text` with each `~mission(Name)` (or `~mission(Name|Other)`) as `[NAME]`.
fn placeholders(text: &str) -> String {
    const START: &str = "~mission(";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(START) {
        let after = &rest[at + START.len()..];
        let Some(end) = after.find(')') else { break };
        out.push_str(&rest[..at]);
        let token = after[..end].split('|').next().unwrap_or_default().trim();
        out.push('[');
        out.push_str(&token.to_uppercase());
        out.push(']');
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out.trim().to_string()
}

fn number(value: &Value) -> Option<i64> {
    match value {
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) if x.is_finite() => Some(x.round() as i64),
        _ => None,
    }
}

/// A number or a `[low, high]` pair as `(low, high)`.
fn span(value: Option<&Value>) -> Option<(i64, i64)> {
    match value? {
        Value::Array(pair) if pair.len() == 2 => {
            let (a, b) = (number(&pair[0])?, number(&pair[1])?);
            Some((a.min(b), a.max(b)))
        }
        other => number(other).map(|n| (n, n)),
    }
}

fn field<'a>(entry: &'a Map, name: &str) -> Option<&'a Value> {
    entry.iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

fn text(entry: &Map, name: &str) -> Option<String> {
    field(entry, name).and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
}

fn rank(entry: &Map, name: &str, reputation: &str) -> Option<Rank> {
    Some(Rank { name: text(entry, name)?, reputation: field(entry, reputation).and_then(number)? })
}

fn gains(rows: &Value) -> Vec<Gain> {
    let Value::Array(rows) = rows else { return Vec::new() };
    rows.iter()
        .filter_map(|row| {
            let row = row.as_object()?;
            // An amount the table does not know (`null`) is a gain of unknown size, not no gain.
            let unknown = matches!(field(row, "wieviel"), Some(Value::Null));
            let (low, high) = if unknown { (0, 0) } else { span(field(row, "wieviel"))? };
            let who = text(row, "wer").unwrap_or_default();
            let kind = text(row, "was").unwrap_or_default();
            (!who.is_empty() || !kind.is_empty()).then_some(Gain { who, kind, low, high, unknown })
        })
        .collect()
}

/// The star systems of a region as the table writes it, in its order.
pub fn systems_of(region: &str) -> Vec<String> {
    let mut systems = Vec::new();
    let mut depth = 0usize;
    let mut part = String::new();
    for c in region.chars().chain([',']) {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let system = part.split('(').next().unwrap_or_default().trim().to_string();
                if !system.is_empty() && !systems.contains(&system) {
                    systems.push(system);
                }
                part.clear();
                continue;
            }
            _ => {}
        }
        part.push(c);
    }
    systems
}

/// The contracts of `table`, `missions` (title key → blueprint names of the catalog; `None` without a catalog) and
/// `titles` (title key → shown title), one per title key, by key.
pub fn contracts(
    table: &reputation::Table,
    missions: Option<&HashMap<String, Vec<String>>>,
    titles: &HashMap<String, String>,
) -> Vec<Contract> {
    fn by_key(map: &Map) -> HashMap<String, &Value> {
        map.iter().map(|(k, v)| (key(k), v)).collect()
    }
    let (terms, parties, regions) = (by_key(&table.terms), by_key(&table.contracts), by_key(&table.regions));
    let blueprints: Option<HashMap<String, &Vec<String>>> =
        missions.map(|m| m.iter().map(|(k, names)| (key(k), names)).collect());
    let keys: BTreeSet<String> = title_keys(table, missions).into_iter().collect();
    keys.into_iter()
        .map(|title_key| {
            let entry = terms.get(&title_key).and_then(|v| v.as_object());
            let region = regions
                .get(&title_key)
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(str::to_string);
            let cooldown = entry.and_then(|e| span(field(e, "warte"))).filter(|(_, high)| *high > 0);
            Contract {
                title: titles.get(&title_key).cloned(),
                giver: entry.and_then(|e| text(e, GIVER)),
                gains: parties.get(&title_key).map(|rows| gains(rows)).unwrap_or_default(),
                cooldown,
                cooldown_spread: cooldown
                    .and(entry.and_then(|e| field(e, "warte_streuung")).and_then(number))
                    .filter(|s| *s > 0),
                shareable: entry.and_then(|e| match field(e, "teilbar") {
                    Some(Value::Bool(b)) => Some(*b),
                    _ => None,
                }),
                systems: region.as_deref().map(systems_of).unwrap_or_default(),
                region,
                rank: entry.and_then(|e| rank(e, "rang", "rep")),
                rank_max: entry.and_then(|e| rank(e, "rang_max", "rep_max")),
                blueprints: blueprints.as_ref().map(|all| {
                    let mut names = all.get(&title_key).map(|names| (*names).clone()).unwrap_or_default();
                    names.sort_by_key(|name| name.to_lowercase());
                    names.dedup();
                    names
                }),
                key: title_key,
            }
        })
        .collect()
}

/// What the list sorts by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Column {
    Giver,
    #[default]
    Title,
    Gain,
    Cooldown,
    Shareable,
    Region,
    Rank,
    Blueprints,
}

impl Column {
    pub const ALL: [Column; 8] = [
        Column::Giver,
        Column::Title,
        Column::Gain,
        Column::Cooldown,
        Column::Shareable,
        Column::Region,
        Column::Rank,
        Column::Blueprints,
    ];
}

/// How the list is filtered and sorted. An empty filter lets every contract through.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// A part of the title, the key, the giver, the region, a party or a blueprint; capitals and umlauts alike.
    pub text: String,
    /// Only the contracts of this giver.
    pub giver: Option<String>,
    /// Only the contracts that play in this star system.
    pub system: Option<String>,
    /// Only the contracts a holder of this rank can take: those whose lowest rank starts at no more reputation than
    /// this rank does and whose highest rank, where one is named, starts at no less. A contract without a known lowest
    /// rank does not pass.
    pub rank: Option<Rank>,
    /// Only the contracts that award a blueprint.
    pub with_blueprint: bool,
    /// Only the contracts that can be shared.
    pub shareable: bool,
    pub sort: Column,
    /// Descending when false; a contract without the value sorted stands last either way.
    pub ascending: bool,
}

impl View {
    /// The usual view: every contract, by title.
    pub fn new() -> View {
        View { ascending: true, ..View::default() }
    }

    /// Sorts by `column`; the column sorted already turns its direction.
    pub fn sort_by(&mut self, column: Column) {
        if self.sort == column {
            self.ascending = !self.ascending;
        } else {
            self.sort = column;
            self.ascending = true;
        }
    }
}

/// Lower case with `ß` as `ss` and the umlauts as `ae`, `oe`, `ue`.
fn fold(text: &str) -> String {
    text.to_lowercase().replace('ß', "ss").replace('ä', "ae").replace('ö', "oe").replace('ü', "ue")
}

fn matches_text(contract: &Contract, wanted: &str) -> bool {
    let has = |text: &str| fold(text).contains(wanted);
    has(&contract.key)
        || contract.title.as_deref().is_some_and(has)
        || contract.giver.as_deref().is_some_and(has)
        || contract.region.as_deref().is_some_and(has)
        || contract.gains.iter().any(|gain| has(&gain.who))
        || contract.blueprints.iter().flatten().any(|name| has(name))
}

/// Whether a holder of the rank `held` can take `contract`; `false` where the contract's lowest rank is unknown.
pub fn takes(contract: &Contract, held: &Rank) -> bool {
    contract.rank.as_ref().is_some_and(|lowest| lowest.reputation <= held.reputation)
        && contract.rank_max.as_ref().is_none_or(|highest| highest.reputation >= held.reputation)
}

/// Whether `contract` passes the filters of `view`.
pub fn passes(contract: &Contract, view: &View) -> bool {
    let wanted = fold(view.text.trim());
    (wanted.is_empty() || matches_text(contract, &wanted))
        && view.giver.as_ref().is_none_or(|giver| contract.giver.as_ref() == Some(giver))
        && view.system.as_ref().is_none_or(|system| contract.systems.contains(system))
        && view.rank.as_ref().is_none_or(|held| takes(contract, held))
        && (!view.with_blueprint || contract.blueprints.as_ref().is_some_and(|names| !names.is_empty()))
        && (!view.shareable || contract.shareable == Some(true))
}

/// How many contracts pass every filter of `view` but its rank, yet name no lowest rank, so that a rank filter leaves
/// them out.
pub fn without_rank(contracts: &[Contract], view: &View) -> usize {
    let other = View { rank: None, ..view.clone() };
    contracts.iter().filter(|contract| contract.rank.is_none() && passes(contract, &other)).count()
}

/// What a contract sorts by: a number and a text that breaks a tie; `None` sorts last.
type SortValue = Option<(i64, String)>;

/// The value of `column` a contract sorts by; `None` sorts last.
fn sort_value(contract: &Contract, column: Column) -> SortValue {
    let word = |text: &Option<String>| text.as_deref().map(|t| (0, fold(t)));
    match column {
        Column::Giver => word(&contract.giver),
        Column::Title => Some((0, fold(contract.title.as_deref().unwrap_or(&contract.key)))),
        Column::Gain => contract.best_gain().map(|gain| (gain, String::new())),
        Column::Cooldown => contract.cooldown.map(|(low, _)| (low, String::new())),
        Column::Shareable => contract.shareable.map(|yes| (i64::from(!yes), String::new())),
        Column::Region => word(&contract.region),
        Column::Rank => contract.rank.as_ref().map(|rank| (rank.reputation, fold(&rank.name))),
        Column::Blueprints => contract.blueprints.as_ref().map(|names| (names.len() as i64, String::new())),
    }
}

/// The places in `contracts` of those `view` lets through, in its order; contracts equal in the column sorted stand
/// by title.
pub fn arrange(contracts: &[Contract], view: &View) -> Vec<usize> {
    let mut shown: Vec<(usize, SortValue, String)> = contracts
        .iter()
        .enumerate()
        .filter(|(_, contract)| passes(contract, view))
        .map(|(at, contract)| {
            (at, sort_value(contract, view.sort), fold(contract.title.as_deref().unwrap_or(&contract.key)))
        })
        .collect();
    shown.sort_by(|(_, a, a_title), (_, b, b_title)| {
        let order = match (a, b) {
            (Some(a), Some(b)) => {
                let order = a.cmp(b);
                if view.ascending { order } else { order.reverse() }
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        order.then_with(|| a_title.cmp(b_title))
    });
    shown.into_iter().map(|(at, ..)| at).collect()
}

/// The values the filters offer: every giver, star system and lowest rank the contracts name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choices {
    /// In alphabetical order.
    pub givers: Vec<String>,
    /// In alphabetical order.
    pub systems: Vec<String>,
    /// By the reputation they start at, then by name; each name once, at its lowest reputation.
    pub ranks: Vec<Rank>,
}

impl Choices {
    pub fn of(contracts: &[Contract]) -> Choices {
        let givers: BTreeSet<&str> = contracts.iter().filter_map(|c| c.giver.as_deref()).collect();
        let systems: BTreeSet<&str> = contracts.iter().flat_map(|c| c.systems.iter().map(String::as_str)).collect();
        let mut ranks: Vec<Rank> = Vec::new();
        for rank in contracts.iter().filter_map(|c| c.rank.as_ref()) {
            match ranks.iter_mut().find(|known| known.name == rank.name) {
                Some(known) => known.reputation = known.reputation.min(rank.reputation),
                None => ranks.push(rank.clone()),
            }
        }
        ranks.sort_by(|a, b| a.reputation.cmp(&b.reputation).then_with(|| fold(&a.name).cmp(&fold(&b.name))));
        Choices {
            givers: givers.into_iter().map(str::to_string).collect(),
            systems: systems.into_iter().map(str::to_string).collect(),
            ranks,
        }
    }
}

#[cfg(test)]
mod tests;
