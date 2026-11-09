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

//! The fixed rewards of contracts in aUEC, as the catalog states them: the field `belohnung` of a contract in the
//! section `angaben` of the reputation table (`auftragsruf.json`), by the key of its title.
//!
//! ⚠ The log names a completed contract by its displayed title only, never with an amount. A title stands for every
//! key of the language files whose text it matches (the variants `_001`, `_002` … each have a key of their own),
//! whether the terms list that key or not. It leads to a reward only when every one of these keys states the same
//! fixed amount; a key without terms, without `belohnung` or with a range (`[min, max]`) among them gives no
//! amount. Nothing is guessed. Few contracts state a fixed reward at all.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::injection::reputation::{CACHE_FILE, key};
use crate::jsonio::{self, Value};
use crate::mission_log::{self, ContractIndex};
use crate::paths::Paths;

/// The section of the reputation table with the terms of a contract.
const TERMS: &str = "angaben";
/// The fixed reward of a contract in its terms.
const REWARD: &str = "belohnung";

/// The fixed rewards and the way from a displayed title to them.
#[derive(Default)]
pub struct Rewards {
    /// Every key of the terms with its fixed reward, or `None` where it states none.
    by_key: HashMap<String, Option<i64>>,
    index: ContractIndex,
}

impl Rewards {
    /// From the reputation table of the data folder and the language files of the installation.
    pub fn load(paths: &Paths) -> Rewards {
        let Ok(data) = jsonio::read_json(&paths.app_file(CACHE_FILE)) else { return Rewards::default() };
        Rewards::of(&data, &mission_log::ini_files(paths))
    }

    /// From a parsed reputation table and the given language files.
    pub fn of(data: &Value, ini_files: &[PathBuf]) -> Rewards {
        let by_key = fixed_rewards(data);
        if by_key.values().all(Option::is_none) {
            return Rewards::default();
        }
        Rewards { index: ContractIndex::build_strict(ini_files), by_key }
    }

    /// No contract with a fixed reward known?
    pub fn is_empty(&self) -> bool {
        self.by_key.values().all(Option::is_none)
    }

    /// The fixed reward of the contract with this displayed title: the one amount all its keys state, or `None`.
    pub fn of_title(&self, title: &str) -> Option<i64> {
        let mut amounts = self.index.candidates(title).into_iter().map(|k| self.by_key.get(&k).copied().flatten());
        let first = amounts.next()??;
        amounts.all(|amount| amount == Some(first)).then_some(first)
    }
}

/// Every key (lower case, without `@`) of the terms → its fixed reward: a whole number of at least 1, else `None`.
fn fixed_rewards(data: &Value) -> HashMap<String, Option<i64>> {
    let Some(Value::Object(terms)) = data.get(TERMS) else { return HashMap::new() };
    terms
        .iter()
        .map(|(title, entry)| {
            let amount = match entry.get(REWARD) {
                Some(Value::Int(text)) => text.parse::<i64>().ok().filter(|amount| *amount > 0),
                _ => None,
            };
            (key(title), amount)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(rewards.of_title("Same Name"), None, "one title of two contracts gives no amount");
        assert_eq!(rewards.of_title("Ranged Pay"), None, "a range is no fixed reward");
        assert_eq!(rewards.of_title("Computed Pay"), None, "without belohnung only the count");
        assert_eq!(rewards.of_title("Unknown Contract"), None);
        assert_eq!(rewards.of_title("Recover the Black Box"), Some(9000), "variants of one text and one amount");
        assert_eq!(rewards.of_title("Mixed Pay"), None, "a fixed and a computed contract of one text give none");
        assert_eq!(rewards.of_title("Lonely Pay"), None, "a twin the terms do not list gives none as well");
        assert_eq!(rewards.of_title("Deliver Gold now"), None, "a pattern of a twin outside the terms ties");
        assert_eq!(rewards.of_title("Menu"), None, "a text that is no contract of the terms");
    }

    #[test]
    fn a_table_without_terms_or_without_a_reward_knows_none() {
        let (_dir, files) = ini("ohne-belohnung", &["Bounty_Title=Kill the Pirate"]);
        let old = table(r#"{"format": 2, "auftraege": {"bounty_title": [{"wer": "x", "was": "y", "wieviel": 5}]}}"#);
        assert!(Rewards::of(&old, &files).is_empty());
        assert_eq!(Rewards::of(&old, &files).of_title("Kill the Pirate"), None);
        let none = table(r#"{"format": 3, "angaben": {"bounty_title": {"rang": 1}}}"#);
        assert!(Rewards::of(&none, &files).is_empty());
    }
}
