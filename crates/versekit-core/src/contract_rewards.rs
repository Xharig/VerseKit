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

//! The rewards of contracts in aUEC, as the catalog states them, by the key of the title of a contract.
//!
//! Three places of the catalog state an amount per key, the first wins:
//! 1. the field `uec` of a contract in the section `missionen` of the catalog file: the amount VerseKit writes into
//!    the text of the contract (`Belohnung: … aUEC`), so what the player read is what is booked;
//! 2. the field `belohnung` of a contract in the section `angaben` of the reputation table (`auftragsruf.json`):
//!    one number, or `[min, max]` where the variants of a title differ;
//! 3. the field `belohnung_ca` there: the lowest base value scmdb lists (`rewardUEC`) for a title whose payout the
//!    game computes. It is always approximate.
//!
//! ⚠ The log names a completed contract by its displayed title only, never with an amount. A title stands for every
//! key of the language files whose text it matches (the variants `_001`, `_002` … each have a key of their own),
//! whether the catalog lists that key or not. It leads to an amount only when every one of these keys states one: a
//! key without an amount gives none. Where the keys state the same single amount, it is exact; where they differ
//! (variants, a range), the lowest amount is given and marked as approximate ([`Reward::approx`]): the contract
//! pays at least this. Nothing is guessed beyond that.
//!
//! Only without any key (the language files are missing, the title is unknown to them) the amounts the catalog
//! states for the ways to a blueprint (`bauplaene` → `q` → `uec`, by the literal title) are taken, by the same rule.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::catalog_cache;
use crate::injection::reputation::{CACHE_FILE, key};
use crate::jsonio::{self, Value};
use crate::mission_log::{self, ContractIndex};
use crate::paths::Paths;

/// The section of the reputation table with the terms of a contract.
const TERMS: &str = "angaben";
/// The fixed reward of a contract in its terms.
const REWARD: &str = "belohnung";
/// The buy-in of a contract in its terms: money the player pays to take it, no reward.
const COST: &str = "buyin";
/// The base payout scmdb lists for a title the game data give no fixed payout for.
const REWARD_BASE: &str = crate::injection::terms::REWARD_BASE;
/// The section of the catalog file with the contracts that give blueprints, and their reward.
const MISSIONS: &str = "missionen";
const MISSION_REWARD: &str = "uec";

/// What a completed contract is booked with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reward {
    /// aUEC, at least 1.
    pub amount: i64,
    /// The title stands for contracts that pay differently: the amount is the lowest of them.
    pub approx: bool,
}

/// The least and the most a key pays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    least: i64,
    most: i64,
    /// Only a base value of scmdb: the payout may be computed, so it never counts as exact.
    base: bool,
}

impl Span {
    fn exact(amount: i64) -> Span {
        Span { least: amount, most: amount, base: false }
    }
}

/// The rewards and the way from a displayed title to them.
#[derive(Default)]
pub struct Rewards {
    /// Every key of the catalog with what it pays, or `None` where it states nothing.
    by_key: HashMap<String, Option<Span>>,
    /// The ways to blueprints by displayed title (lower case) with their amounts; the second source.
    by_title: HashMap<String, Vec<i64>>,
    /// Every key of the terms with what it costs to take it (`buyin`), or `None` where it states none.
    costs: HashMap<String, Option<Span>>,
    index: ContractIndex,
}

/// A file as it stood when the rewards were made of it: its path, its length and when it was written.
type Stamp = (PathBuf, Option<(u64, std::time::SystemTime)>);

/// The rewards made last and the files they were made of.
static MADE: std::sync::Mutex<Option<(Vec<Stamp>, std::sync::Arc<Rewards>)>> = std::sync::Mutex::new(None);

/// The files the rewards are made of.
fn source_files(paths: &Paths) -> Vec<PathBuf> {
    let mut files = vec![PathBuf::from(paths.app_file(CACHE_FILE)), PathBuf::from(catalog_cache::current_file(paths))];
    files.extend(mission_log::ini_files(paths));
    files
}

/// Are the rewards [`Rewards::load_kept`] keeps made of the files as they are now (then it answers at once)?
pub fn kept_is_current(paths: &Paths) -> bool {
    let now = stamps(&source_files(paths));
    MADE.lock().ok().is_some_and(|made| made.as_ref().is_some_and(|(before, _)| *before == now))
}

fn stamps(files: &[PathBuf]) -> Vec<Stamp> {
    files
        .iter()
        .map(|file| {
            let meta = std::fs::metadata(file).ok();
            (file.clone(), meta.and_then(|m| Some((m.len(), m.modified().ok()?))))
        })
        .collect()
}

impl Rewards {
    /// [`Rewards::load`], made again only when the reputation table, the catalog or a language file changed since
    /// the last call: the index over the language files takes seconds.
    pub fn load_kept(paths: &Paths) -> std::sync::Arc<Rewards> {
        let now = stamps(&source_files(paths));
        if let Ok(made) = MADE.lock()
            && let Some((before, rewards)) = made.as_ref()
            && *before == now
        {
            return rewards.clone();
        }
        let rewards = std::sync::Arc::new(Rewards::load(paths));
        if let Ok(mut made) = MADE.lock() {
            *made = Some((now, rewards.clone()));
        }
        rewards
    }

    /// From the reputation table and the catalog of the data folder and the language files of the installation.
    pub fn load(paths: &Paths) -> Rewards {
        let terms = jsonio::read_json(&paths.app_file(CACHE_FILE)).unwrap_or(Value::Null);
        let catalog = jsonio::read_json(&catalog_cache::current_file(paths)).unwrap_or(Value::Null);
        Rewards::of_sources(&terms, &catalog, &mission_log::ini_files(paths))
    }

    /// From a parsed reputation table and the given language files.
    pub fn of(data: &Value, ini_files: &[PathBuf]) -> Rewards {
        Rewards::of_sources(data, &Value::Null, ini_files)
    }

    /// From a parsed reputation table, a parsed catalog file and the given language files.
    pub fn of_sources(terms: &Value, catalog: &Value, ini_files: &[PathBuf]) -> Rewards {
        let mut by_key = terms_rewards(terms);
        for (key, span) in mission_rewards(catalog) {
            by_key.insert(key, Some(span));
        }
        let by_title = way_rewards(catalog);
        let costs = terms_costs(terms);
        if by_key.values().all(Option::is_none) && by_title.is_empty() && costs.values().all(Option::is_none) {
            return Rewards::default();
        }
        Rewards { index: ContractIndex::build_strict(ini_files), by_key, by_title, costs }
    }

    /// No contract with a reward or a cost known?
    pub fn is_empty(&self) -> bool {
        self.by_key.values().all(Option::is_none)
            && self.by_title.is_empty()
            && self.costs.values().all(Option::is_none)
    }

    /// What it costs to take the contract with this displayed title (its buy-in, paid by whoever takes it): exact,
    /// or the lowest amount marked approximate where variants differ; `None` where the terms state none.
    pub fn cost_of_title(&self, title: &str) -> Option<Reward> {
        let keys = self.index.candidates(title);
        let spans: Option<Vec<Span>> = keys.iter().map(|k| self.costs.get(k).copied().flatten()).collect();
        let spans = spans.filter(|_| !keys.is_empty())?;
        Some(summed_up(spans.into_iter()))
    }

    /// What the contract with this displayed title pays: exact, or the lowest amount marked approximate; `None`
    /// where the catalog states none for it.
    pub fn reward_of_title(&self, title: &str) -> Option<Reward> {
        let keys = self.index.candidates(title);
        if keys.is_empty() {
            let amounts = self.by_title.get(&mission_log::clean(title).to_lowercase())?;
            return Some(summed_up(amounts.iter().map(|amount| Span::exact(*amount))));
        }
        let spans: Option<Vec<Span>> = keys.iter().map(|k| self.by_key.get(k).copied().flatten()).collect();
        Some(summed_up(spans?.into_iter()))
    }

    /// The fixed reward of the contract with this displayed title: only an exact one, else `None`.
    pub fn of_title(&self, title: &str) -> Option<i64> {
        self.reward_of_title(title).filter(|reward| !reward.approx).map(|reward| reward.amount)
    }
}

/// What the spans of the keys of a title come to: their lowest amount, exact where they all state that one amount.
fn summed_up(spans: impl Iterator<Item = Span>) -> Reward {
    let (mut low, mut high, mut base) = (i64::MAX, i64::MIN, false);
    for span in spans {
        low = low.min(span.least);
        high = high.max(span.most);
        base |= span.base;
    }
    Reward { amount: low, approx: base || low != high }
}

/// A whole number of at least 1.
fn amount_of(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::Int(text)) => text.parse::<i64>().ok().filter(|amount| *amount > 0),
        _ => None,
    }
}

/// Every key (lower case, without `@`) of the terms → what it pays: one amount, or `[min, max]`; else `None`.
fn terms_rewards(data: &Value) -> HashMap<String, Option<Span>> {
    let Some(Value::Object(terms)) = data.get(TERMS) else { return HashMap::new() };
    terms
        .iter()
        .map(|(title, entry)| {
            let span = match entry.get(REWARD) {
                Some(Value::Array(pair)) if pair.len() == 2 => amount_of(pair.first())
                    .zip(amount_of(pair.get(1)))
                    .map(|(a, b)| Span { least: a.min(b), most: a.max(b), base: false }),
                other => amount_of(other).map(Span::exact),
            }
            // A payout the game computes: scmdb's base value, "at least".
            .or_else(|| amount_of(entry.get(REWARD_BASE)).map(|amount| Span { base: true, ..Span::exact(amount) }));
            (key(title), span)
        })
        .collect()
}

/// Every key (lower case, without `@`) of the terms → what it costs to take it (`buyin`): one amount or `[min, max]`.
fn terms_costs(data: &Value) -> HashMap<String, Option<Span>> {
    let Some(Value::Object(terms)) = data.get(TERMS) else { return HashMap::new() };
    terms
        .iter()
        .map(|(title, entry)| {
            let span = match entry.get(COST) {
                Some(Value::Array(pair)) if pair.len() == 2 => amount_of(pair.first())
                    .zip(amount_of(pair.get(1)))
                    .map(|(a, b)| Span { least: a.min(b), most: a.max(b), base: false }),
                other => amount_of(other).map(Span::exact),
            };
            (key(title), span)
        })
        .collect()
}

/// Every contract of the catalog that states a reward (`uec`) by its key (lower case, without `@`).
fn mission_rewards(catalog: &Value) -> Vec<(String, Span)> {
    let Some(Value::Object(missions)) = catalog.get(MISSIONS) else { return Vec::new() };
    missions
        .iter()
        .filter_map(|(title, entry)| {
            let amount = amount_of(entry.get(MISSION_REWARD))?;
            Some((key(title), Span::exact(amount)))
        })
        .collect()
}

/// The amounts the ways to the blueprints of the catalog state, by the title of their contract (cleaned, lower case).
fn way_rewards(catalog: &Value) -> HashMap<String, Vec<i64>> {
    let mut out: HashMap<String, Vec<i64>> = HashMap::new();
    for blueprint in &catalog_cache::parse(catalog).blueprints {
        for way in &blueprint.ways {
            let (title, Some(reward)) = (mission_log::clean(&way.contract).to_lowercase(), way.reward) else { continue };
            if title.is_empty() || reward < 1 {
                continue;
            }
            let amounts = out.entry(title).or_default();
            if !amounts.contains(&reward) {
                amounts.push(reward);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kept_rewards_are_made_again_only_when_a_source_file_changed() {
        let dir = std::env::temp_dir().join(format!("versekit-belohnung-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        let text = dir.to_string_lossy().into_owned();
        let host = crate::paths::Host::new(crate::pypath::Flavor::native(), &[(crate::envvars::ENV_HOME, &text)], &text);
        let paths = Paths::new(host);
        let table = paths.app_file(CACHE_FILE);
        std::fs::create_dir_all(std::path::Path::new(&table).parent().unwrap()).unwrap();
        std::fs::write(&table, r#"{"angaben": {"a_title": {"belohnung": 1000}}}"#).unwrap();

        let first = Rewards::load_kept(&paths);
        assert!(kept_is_current(&paths));
        let again = Rewards::load_kept(&paths);
        assert!(std::sync::Arc::ptr_eq(&first, &again), "unchanged files give the kept rewards");

        std::fs::write(&table, r#"{"angaben": {"a_title": {"belohnung": 25000}}}"#).unwrap();
        assert!(!kept_is_current(&paths), "a changed table is noticed");
        let changed = Rewards::load_kept(&paths);
        assert!(!std::sync::Arc::ptr_eq(&first, &changed), "a changed table makes the rewards again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A language file with these lines, in a folder of its own.
    fn ini(tag: &str, lines: &[&str]) -> (crate::testdir::Dir, Vec<PathBuf>) {
        let dir = crate::testdir::Dir::new(tag);
        let file = dir.path().join("global.ini");
        std::fs::write(&file, format!("\u{feff}{}\r\n", lines.join("\r\n"))).unwrap();
        (dir, vec![file])
    }

    fn table(text: &str) -> Value {
        jsonio::loads(text).expect("json")
    }

    fn exact(amount: i64) -> Option<Reward> {
        Some(Reward { amount, approx: false })
    }

    fn about(amount: i64) -> Option<Reward> {
        Some(Reward { amount, approx: true })
    }

    #[test]
    fn a_title_of_one_key_gets_its_fixed_reward_and_nothing_else_is_guessed() {
        let (_dir, files) = ini(
            "belohnung",
            &[
                "Bounty_Title=Kill the Pirate",
                "Haul_Title,P=Haul ~mission(Cargo) to ~mission(Location)",
                "Twin_A_Title=Same Name",
                "Twin_B_Title=Same Name",
                "Ranged_Title=Ranged Pay",
                "Computed_Title=Computed Pay",
                "Hockrow_Blackbox_Space_E_Title_001=Recover the Black Box",
                "Hockrow_Blackbox_Space_E_Title_002=Recover the Black Box",
                "Mixed_Fixed_Title=Mixed Pay",
                "Mixed_Computed_Title=Mixed Pay",
                "Lonely_Fixed_Title=Lonely Pay",
                "Lonely_Other_Title=Lonely Pay",
                "Deliver_Fixed_Title=Deliver ~mission(Cargo) now",
                "Deliver_Other_Title=Deliver ~mission(Item) now",
                "Some_Menu_Label=Menu",
            ],
        );
        let data = table(
            r#"{"format": 3, "angaben": {
                "@Bounty_Title": {"belohnung": 15000, "rang": 2},
                "haul_title": {"belohnung": 42000},
                "twin_a_title": {"belohnung": 1000},
                "twin_b_title": {"belohnung": 2000},
                "ranged_title": {"belohnung": [1000, 5000]},
                "computed_title": {"rang": 1},
                "zero_title": {"belohnung": 0},
                "hockrow_blackbox_space_e_title_001": {"belohnung": 9000},
                "hockrow_blackbox_space_e_title_002": {"belohnung": 9000},
                "mixed_fixed_title": {"belohnung": 3000},
                "mixed_computed_title": {"rang": 1},
                "lonely_fixed_title": {"belohnung": 5000},
                "deliver_fixed_title": {"belohnung": 100}
            }}"#,
        );
        let rewards = Rewards::of(&data, &files);
        assert!(!rewards.is_empty());
        assert_eq!(rewards.of_title("Kill the Pirate"), Some(15_000), "the key is matched in any case");
        assert_eq!(rewards.of_title("Haul Gold to Orison"), Some(42_000), "a title with placeholders");
        assert_eq!(rewards.reward_of_title("Same Name"), about(1000), "two keys of one title: at least the lower one");
        assert_eq!(rewards.of_title("Same Name"), None, "an approximate amount is no fixed reward");
        assert_eq!(rewards.reward_of_title("Ranged Pay"), about(1000), "a range gives its lower end, marked");
        assert_eq!(rewards.reward_of_title("Computed Pay"), None, "without belohnung only the count");
        assert_eq!(rewards.reward_of_title("Unknown Contract"), None);
        assert_eq!(rewards.reward_of_title("Recover the Black Box"), exact(9000), "variants of one text and one amount");
        assert_eq!(rewards.reward_of_title("Mixed Pay"), None, "a fixed and a computed contract of one text give none");
        assert_eq!(rewards.reward_of_title("Lonely Pay"), None, "a twin the terms do not list gives none as well");
        assert_eq!(rewards.reward_of_title("Deliver Gold now"), None, "a pattern of a twin outside the terms ties");
        assert_eq!(rewards.reward_of_title("Menu"), None, "a text that is no contract of the terms");
    }

    #[test]
    fn a_table_without_terms_or_without_a_reward_knows_none() {
        let (_dir, files) = ini("ohne-belohnung", &["Bounty_Title=Kill the Pirate"]);
        let old = table(r#"{"format": 2, "auftraege": {"bounty_title": [{"wer": "x", "was": "y", "wieviel": 5}]}}"#);
        assert!(Rewards::of(&old, &files).is_empty());
        assert_eq!(Rewards::of(&old, &files).reward_of_title("Kill the Pirate"), None);
        let none = table(r#"{"format": 3, "angaben": {"bounty_title": {"rang": 1}}}"#);
        assert!(Rewards::of(&none, &files).is_empty());
    }

    #[test]
    fn the_amount_the_text_states_beats_the_terms_and_a_title_without_an_entry_stays_a_count() {
        let (_dir, files) = ini(
            "uec",
            &[
                "Hunt_Title=Hunt the Wolf",
                "Escort_Title_001=Escort the Convoy",
                "Escort_Title_002=Escort the Convoy",
                "Dig_Title=Dig for Gold",
                "Blank_Title=Blank Job",
            ],
        );
        let terms = table(r#"{"format": 3, "angaben": {"hunt_title": {"belohnung": 111}, "dig_title": {"rang": 1}}}"#);
        let catalog = table(
            r#"{"missionen": {
                "Hunt_Title": {"bp": ["x"], "uec": 70750},
                "Escort_Title_001": {"bp": ["x"], "uec": 82750},
                "Escort_Title_002": {"bp": ["x"], "uec": 90000},
                "Blank_Title": {"bp": ["x"]}
            }}"#,
        );
        let rewards = Rewards::of_sources(&terms, &catalog, &files);
        assert_eq!(rewards.reward_of_title("Hunt the Wolf"), exact(70_750), "the amount VerseKit writes into the text");
        assert_eq!(rewards.reward_of_title("Escort the Convoy"), about(82_750), "variants that differ: the lowest, ca.");
        assert_eq!(rewards.reward_of_title("Dig for Gold"), None, "a contract with no entry stays a count");
        assert_eq!(rewards.reward_of_title("Blank Job"), None, "an entry without uec stays a count");
    }

    #[test]
    fn a_base_value_of_scmdb_is_always_approximate_and_a_fixed_payout_beats_it() {
        let (_dir, files) = ini(
            "belohnung-ca",
            &["Spread_Title=Silent Strike", "Fixed_Title=Fixed Job", "Bare_Title=Bare Job"],
        );
        let terms = table(
            r#"{"format": 3, "angaben": {
                "spread_title": {"belohnung_ca": 34000},
                "fixed_title": {"belohnung": 9000, "belohnung_ca": 100},
                "bare_title": {"rang": 1}
            }}"#,
        );
        let rewards = Rewards::of(&terms, &files);
        assert_eq!(rewards.reward_of_title("Silent Strike"), about(34_000), "one base value is still no exact payout");
        assert_eq!(rewards.of_title("Silent Strike"), None);
        assert_eq!(rewards.reward_of_title("Fixed Job"), exact(9000));
        assert_eq!(rewards.reward_of_title("Bare Job"), None, "no entry, no amount");
    }

    #[test]
    fn a_buy_in_is_a_cost_and_never_a_reward() {
        let (_dir, files) = ini(
            "buyin",
            &["Heist_Title=Goods for the Getting", "Salvage_Title_001=Salvage Job", "Salvage_Title_002=Salvage Job", "Free_Title=Free Job"],
        );
        let terms = table(
            r#"{"format": 3, "angaben": {
                "heist_title": {"buyin": 33000},
                "salvage_title_001": {"buyin": 250},
                "salvage_title_002": {"buyin": [5000, 10000]},
                "free_title": {"warte": 15}
            }}"#,
        );
        let rewards = Rewards::of(&terms, &files);
        assert!(!rewards.is_empty());
        assert_eq!(rewards.cost_of_title("Goods for the Getting"), exact(33_000));
        assert_eq!(rewards.reward_of_title("Goods for the Getting"), None, "what it costs is no reward");
        assert_eq!(rewards.cost_of_title("Salvage Job"), about(250), "variants that differ: the lowest, ca.");
        assert_eq!(rewards.cost_of_title("Free Job"), None);
        assert_eq!(rewards.cost_of_title("Unknown"), None);
    }

    #[test]
    fn without_a_language_file_the_ways_of_the_catalog_give_the_amount() {
        let catalog = table(
            r#"{"bauplaene": {
                "a": {"n": "Alpha", "q": [{"auftrag": "Hunt the Wolf", "uec": 5000}, {"auftrag": "Two Prices", "uec": 100}]},
                "b": {"n": "Beta", "q": [{"auftrag": "Hunt the Wolf", "uec": 5000}, {"auftrag": "Two Prices", "uec": 300}]}
            }}"#,
        );
        let rewards = Rewards::of_sources(&Value::Null, &catalog, &[]);
        assert_eq!(rewards.reward_of_title("Hunt the Wolf"), exact(5000));
        assert_eq!(rewards.reward_of_title("Two Prices"), about(100));
        assert_eq!(rewards.reward_of_title("Nothing"), None);
    }
}
