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

//! From which reputation a rank begins, read from the reputation scopes and standings of the p4kx catalog
//! (`reputationScopes`, `reputationStandings`): text key of the rank's name → `minReputation`, for
//! `versekit_core::injection::ranks`.
//!
//! A scope counts whole or not at all: every standing of it that has a name with an English text, ordered by
//! threshold, and no two with the same threshold. A key that two standings give different thresholds is left out,
//! since the menu shows one text for both.

use std::collections::HashMap;

use serde::Deserialize;

/// The record path part that selects the reputation scopes, standings and contexts.
pub(crate) const REPUTATION_RECORDS: &str = "libs/foundry/records/reputation/";

/// `versekit_core::injection::ranks::refresh` with the archive `versekit_core::paths` finds: read only when the
/// stored thresholds do not fit `game_version`, and not while the game runs (the old ones stay until the next
/// time); a failed reading goes to the error log and keeps the old ones.
pub fn refresh(paths: &versekit_core::paths::Paths, game_version: &str) -> usize {
    versekit_core::injection::ranks::refresh(paths, game_version, &|| {
        if !versekit_core::archive_lock::may_read(paths) {
            return None;
        }
        let file = paths.archive_file()?;
        let read = crate::GameData::open_file(&file).and_then(|game| game.rank_thresholds());
        read.inspect_err(|error| versekit_core::errors::record("rank_thresholds.archive", error)).ok()
    })
}

/// Text key (without `@`) → threshold, in the order of the scopes and their standings.
///
/// # Errors
///
/// When the bytes are no catalog JSON.
pub fn thresholds(catalog: &[u8]) -> Result<Vec<(String, i64)>, serde_json::Error> {
    let catalog: RawCatalog = serde_json::from_slice(catalog)?;
    let standings: HashMap<&str, &RawStanding> =
        catalog.reputation_standings.iter().map(|standing| (standing.guid.as_str(), standing)).collect();
    let mut found: Vec<(String, i64)> = Vec::new();
    // A key that any two standings give different thresholds, counted scope or not.
    let mut seen: HashMap<String, i64> = HashMap::new();
    let mut doubtful: Vec<String> = Vec::new();
    for standing in &catalog.reputation_standings {
        let (Some(key), Some(value)) = (standing.display_name.as_ref().and_then(key_of), standing.min_reputation)
        else {
            continue;
        };
        let lower = key.to_ascii_lowercase();
        if seen.get(&lower).is_some_and(|known| *known != value) {
            doubtful.push(lower.clone());
        }
        seen.entry(lower).or_insert(value);
    }
    for scope in &catalog.reputation_scopes {
        let mut named: Vec<(String, i64)> = scope
            .standing_guids
            .iter()
            .filter_map(|guid| standings.get(guid.as_str()))
            .filter_map(|standing| Some((key_of(standing.display_name.as_ref()?)?, standing.min_reputation?)))
            .collect();
        named.sort_by_key(|(_, value)| *value);
        if named.is_empty() || named.windows(2).any(|pair| pair[1].1 == pair[0].1) {
            continue;
        }
        for (key, value) in named {
            match found.iter().find(|(known, _)| known.eq_ignore_ascii_case(&key)) {
                Some((_, known)) if *known != value => doubtful.push(key.to_ascii_lowercase()),
                Some(_) => {}
                None => found.push((key, value)),
            }
        }
    }
    found.retain(|(key, _)| !doubtful.contains(&key.to_ascii_lowercase()));
    Ok(found)
}

/// The text key of a game text that has an English text, without the leading `@`.
fn key_of(text: &RawText) -> Option<String> {
    text.en.as_deref().filter(|en| !en.trim().is_empty())?;
    let key = text.key.as_deref()?.trim_start_matches('@');
    (!key.is_empty()).then(|| key.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCatalog {
    #[serde(default)]
    reputation_scopes: Vec<RawScope>,
    #[serde(default)]
    reputation_standings: Vec<RawStanding>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawScope {
    #[serde(default)]
    standing_guids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStanding {
    guid: String,
    display_name: Option<RawText>,
    min_reputation: Option<i64>,
}

#[derive(Deserialize)]
struct RawText {
    key: Option<String>,
    en: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::thresholds;

    fn standing(guid: &str, key: &str, en: Option<&str>, min: i64) -> String {
        let en = en.map_or_else(|| "null".to_string(), |en| format!("\"{en}\""));
        format!(
            r#"{{"guid": "{guid}", "displayName": {{"key": "@{key}", "en": {en}, "de": null}}, "minReputation": {min}}}"#
        )
    }

    fn catalog(scopes: &[&[&str]], standings: &[String]) -> Vec<u8> {
        let scopes: Vec<String> = scopes
            .iter()
            .map(|guids| {
                format!(
                    r#"{{"standingGuids": [{}]}}"#,
                    guids.iter().map(|g| format!("\"{g}\"")).collect::<Vec<_>>().join(",")
                )
            })
            .collect();
        format!(r#"{{"reputationScopes": [{}], "reputationStandings": [{}]}}"#, scopes.join(","), standings.join(","))
            .into_bytes()
    }

    #[test]
    fn a_scope_counts_whole_and_rising_and_a_disputed_key_is_left_out() {
        let standings = [
            standing("a0", "Barter_Rank0", Some("Nothing"), 0),
            standing("a1", "Barter_Rank1", Some("Armour"), 340),
            // A placeholder without English text does not count.
            standing("a2", "Barter_Placeholder", None, 50),
            standing("b0", "Tech_Rank0", Some("Rookie"), 0),
            standing("b1", "Tech_Rank1", Some("Pro"), 0),
            standing("c0", "Shared_Rank1", Some("Member"), 800),
            standing("d0", "shared_rank1", Some("Member"), 900),
            standing("e0", "Same_Rank1", Some("Same"), 5),
        ];
        // Scope a is stored from the top down, as Wikelo's is.
        let table = thresholds(&catalog(
            &[&["a1", "a2", "missing", "a0"], &["b0", "b1"], &["c0", "e0"], &["d0"], &["e0"]],
            &standings,
        ))
        .unwrap();
        assert_eq!(
            table,
            [("Barter_Rank0".to_string(), 0), ("Barter_Rank1".to_string(), 340), ("Same_Rank1".to_string(), 5)]
        );
    }

    #[test]
    fn a_scope_with_two_equal_thresholds_gives_nothing() {
        let standings = [standing("a0", "R0", Some("x"), 10), standing("a1", "R1", Some("y"), 10)];
        assert!(thresholds(&catalog(&[&["a0", "a1"]], &standings)).unwrap().is_empty());
    }
}
