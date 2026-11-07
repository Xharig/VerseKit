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

//! Which entries a field suggests while typing: a short, sensible list
//! instead of everything that contains the letters.
//!
//! New in v5, no counterpart in the Python version. The order, best first:
//! the name starts with the input → a word in the name starts with it → the
//! name contains it → the start of a word is a small typo away. Within one
//! tier shorter names come first. Case does not matter.

use std::ops::Range;

/// Suggestions start with the second typed character.
pub const MIN_CHARS: usize = 2;
/// At most this many suggestions are shown; the rest is only counted.
pub const MAX_SHOWN: usize = 8;

/// How well an entry matches, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// The name starts with the input.
    Start,
    /// A later word in the name starts with the input.
    WordStart,
    /// The input stands somewhere inside the name.
    Inside,
    /// The start of a word is a small typo away from the input.
    Typo,
}

/// One suggested entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Position in the list of entries.
    pub index: usize,
    pub tier: Tier,
    /// Byte range of the typed part inside the entry; empty for typos.
    pub matched: Range<usize>,
}

/// What a field suggests for an input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestions {
    /// At most [`MAX_SHOWN`] hits, best first.
    pub hits: Vec<Hit>,
    /// How many further entries match but are not shown.
    pub more: usize,
}

/// How many typos an input of `chars` characters may contain: none up to
/// three characters (`bus` would match half the list), one up to seven,
/// two from eight on.
pub fn allowed_typos(chars: usize) -> usize {
    match chars {
        0..=3 => 0,
        4..=7 => 1,
        _ => 2,
    }
}

/// Suggestions from `entries` for the input `query`.
pub fn suggest<S: AsRef<str>>(query: &str, entries: &[S]) -> Suggestions {
    let query: Vec<char> = query.trim().chars().map(fold).collect();
    if query.len() < MIN_CHARS {
        return Suggestions::default();
    }
    let mut found: Vec<(Hit, usize, usize, String)> = entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let entry = entry.as_ref();
            let (tier, matched, typos) = rank(&query, entry)?;
            let key: String = entry.chars().map(fold).collect();
            Some((Hit { index, tier, matched }, typos, entry.chars().count(), key))
        })
        .collect();
    found.sort_by(|a, b| (a.0.tier, a.1, a.2, &a.3).cmp(&(b.0.tier, b.1, b.2, &b.3)));
    let more = found.len().saturating_sub(MAX_SHOWN);
    let hits = found.into_iter().take(MAX_SHOWN).map(|(hit, ..)| hit).collect();
    Suggestions { hits, more }
}

/// Lower case, one character for one, so that positions stay the same.
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The best tier of `entry` for `query`, the byte range of the match and the
/// number of typos.
fn rank(query: &[char], entry: &str) -> Option<(Tier, Range<usize>, usize)> {
    let chars: Vec<(usize, char)> = entry.char_indices().map(|(i, c)| (i, fold(c))).collect();
    let n = query.len();
    let byte_at = |k: usize| chars.get(k).map_or(entry.len(), |&(i, _)| i);
    let word_starts_at = |k: usize| k == 0 || !chars[k - 1].1.is_alphanumeric();

    let mut best: Option<(Tier, usize)> = None;
    for start in 0..=chars.len().saturating_sub(n) {
        let window = chars.get(start..start + n).unwrap_or_default();
        if window.len() == n && window.iter().map(|&(_, c)| c).eq(query.iter().copied()) {
            let tier = match start {
                0 => Tier::Start,
                k if word_starts_at(k) => Tier::WordStart,
                _ => Tier::Inside,
            };
            if best.is_none_or(|(t, _)| tier < t) {
                best = Some((tier, start));
            }
        }
    }
    if let Some((tier, start)) = best {
        return Some((tier, byte_at(start)..byte_at(start + n), 0));
    }

    let allowed = allowed_typos(n);
    if allowed == 0 {
        return None;
    }
    let mut nearest = usize::MAX;
    for start in (0..chars.len()).filter(|&k| word_starts_at(k) && chars[k].1.is_alphanumeric()) {
        for len in n - 1..=n + 1 {
            let Some(part) = chars.get(start..start + len) else { break };
            let part: Vec<char> = part.iter().map(|&(_, c)| c).collect();
            nearest = nearest.min(distance(query, &part));
        }
    }
    (nearest <= allowed).then_some((Tier::Typo, 0..0, nearest))
}

/// Edit distance in which two swapped neighbours count as one typo.
pub fn distance(a: &[char], b: &[char]) -> usize {
    let mut rows: Vec<Vec<usize>> = (0..=a.len()).map(|i| (0..=b.len()).map(|j| i.max(j)).collect()).collect();
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1).min(rows[i][j - 1] + 1).min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRIES: [&str; 14] = [
        "Super Hornet",
        "Buzzard",
        "Buccaneer",
        "Bulwark Rüstung",
        "Arclight Pistole",
        "Cutlass Black",
        "Thornet Helm",
        "Burst Gewehr",
        "Bumblebee",
        "Buran",
        "Bushwhacker",
        "Butterfly",
        "Buoy",
        "Abus Tank",
    ];

    fn names(found: &Suggestions) -> Vec<&'static str> {
        found.hits.iter().map(|h| ENTRIES[h.index]).collect()
    }

    #[test]
    fn the_example_buz_brings_buzzard() {
        let found = suggest("buz", &ENTRIES);
        assert_eq!(names(&found), ["Buzzard"]);
        assert_eq!(found.hits[0].matched, 0..3);
    }

    #[test]
    fn never_more_than_eight_and_the_rest_is_counted() {
        let found = suggest("bu", &ENTRIES);
        assert_eq!(found.hits.len(), 8);
        // 9 names start with `bu`, `Abus Tank` has it inside.
        assert_eq!(found.more, 2);
        assert!(found.hits.iter().all(|h| h.tier == Tier::Start), "{:?}", names(&found));
        // Shorter first: Buoy and Buran before the long ones.
        assert_eq!(&names(&found)[..2], ["Buoy", "Buran"]);

        let many: Vec<String> = (0..10_000).map(|i| format!("Bu {i}")).collect();
        let found = suggest("bu", &many);
        assert_eq!((found.hits.len(), found.more), (8, 10_000 - 8));
    }

    #[test]
    fn start_before_word_start_before_inside() {
        let found = suggest("hor", &ENTRIES);
        assert_eq!(names(&found), ["Super Hornet", "Thornet Helm"]);
        assert_eq!(found.hits[0].tier, Tier::WordStart);
        assert_eq!(found.hits[1].tier, Tier::Inside);
        assert_eq!(found.hits[0].matched, 6..9);
    }

    #[test]
    fn one_entry_appears_once_with_its_best_tier() {
        let entries = ["Hornet Hornet", "Horn"];
        let found = suggest("hornet", &entries);
        assert_eq!(found.hits.len(), 1);
        assert_eq!(found.hits[0].tier, Tier::Start);
    }

    #[test]
    fn too_short_inputs_suggest_nothing() {
        assert_eq!(suggest("b", &ENTRIES), Suggestions::default());
        assert_eq!(suggest("  ", &ENTRIES), Suggestions::default());
    }

    #[test]
    fn case_and_umlauts() {
        let found = suggest("RÜS", &ENTRIES);
        assert_eq!(names(&found), ["Bulwark Rüstung"]);
        let entry = ENTRIES[found.hits[0].index];
        assert_eq!(&entry[found.hits[0].matched.clone()], "Rüs");
    }

    #[test]
    fn small_typos_are_forgiven_only_in_longer_inputs() {
        // One letter missing, two swapped.
        assert_eq!(names(&suggest("bucaneer", &ENTRIES)), ["Buccaneer"]);
        assert_eq!(names(&suggest("buzzrad", &ENTRIES)), ["Buzzard"]);
        assert_eq!(suggest("buzzrad", &ENTRIES).hits[0].tier, Tier::Typo);
        // Three letters: no typos, `bux` finds nothing.
        assert!(suggest("bux", &ENTRIES).hits.is_empty());
        // A typo never outranks a real match.
        let found = suggest("cutl", &["Cutl", "Cuttl"]);
        assert_eq!(found.hits[0].tier, Tier::Start);
    }

    #[test]
    fn distance_counts_swaps_as_one() {
        let d = |a: &str, b: &str| distance(&a.chars().collect::<Vec<_>>(), &b.chars().collect::<Vec<_>>());
        assert_eq!(d("buzzard", "buzzard"), 0);
        assert_eq!(d("buzzrad", "buzzard"), 1);
        assert_eq!(d("bucaneer", "buccaneer"), 1);
        assert_eq!(d("", "abc"), 3);
        assert_eq!(allowed_typos(3), 0);
        assert_eq!(allowed_typos(4), 1);
        assert_eq!(allowed_typos(8), 2);
    }
}
