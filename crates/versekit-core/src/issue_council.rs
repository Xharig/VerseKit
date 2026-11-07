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

//! Searching a contract in the Issue Council, in case it is bugged: only the
//! address of the search is built, nothing is fetched. Like
//! `issue_council.py`.
//!
//! The Issue Council is English, the log holds the title as the game showed
//! it. So: displayed title → text key over the `global.ini` of the game
//! folder, key → English title from `titel-original.json` (read from the
//! `Data.p4k` once), with English placeholder values filled in; without a key
//! a guess from the English words in the title; else the cleaned title.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;

use crate::jsonio::{self, Value};
use crate::mission_log::{self, PLACEHOLDER, pieces, squeeze, weak, weight};
use crate::paths::Paths;
use crate::text::{decode_ignore, py_strip};

pub const BASE_URL: &str = "https://issue-council.robertsspaceindustries.com/projects/STAR-CITIZEN/issues";
/// The players' forum, for everything without a report in the Issue Council.
pub const SPECTRUM_URL: &str = "https://robertsspaceindustries.com/spectrum/community/SC";
/// The English contract titles and the English vocabulary of the game.
pub const TITLES_FILE: &str = "titel-original.json";

/// Every mark in square brackets with a trailing `*`, `!` or `?`.
static BRACKETS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\][*!?]*").expect("brackets"));
/// The game's colour tags, left empty once the brackets are gone.
static EMPHASIS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)</?EM\d*>").expect("emphasis"));
/// Empty parentheses a removed placeholder leaves behind.
static EMPTY_PARENS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(&format!(r"\({}*\)", crate::text::PY_SPACE)).expect("parens"));
static PLACEHOLDER_NAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~mission\(([^)]*)\)").expect("name"));
/// Placeholders for people: different in every contract, never a help.
static PERSON: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)name|target").expect("person"));
static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-zÀ-ÿ][A-Za-zÀ-ÿ'’-]*").expect("word"));

/// What is stripped from the edges once a placeholder is out.
const EDGE: &[char] = &[' ', '\t', ':', ';', ',', '.', '-', '–', '—', '|', '/'];
/// Joining words that point at nothing without the placeholder behind them.
const DANGLING: [&str; 12] = ["at", "in", "on", "near", "from", "for", "to", "of", "the", "and", "with", "by"];
/// Where a placeholder stood while tidying up.
const GAP: &str = "\u{0}";
/// So many English words a foreign title must hold to be searched by them.
const MIN_GUESS_WORDS: usize = 2;

fn edge(word: &str) -> &str {
    word.trim_matches(EDGE)
}

/// The title without marks, emphasis and placeholders (`clean`).
///
/// ⚠ A joining word only goes where a placeholder stood: „Crew Hasn't
/// Checked In“ ends on „In“ by itself and keeps it.
pub fn clean(title: &str) -> String {
    let text = mission_log::clean(title);
    let text = BRACKETS.replace_all(&text, " ");
    let text = EMPHASIS.replace_all(&text, " ");
    let text = PLACEHOLDER.replace_all(&text, format!(" {GAP} ").as_str());
    let text = EMPTY_PARENS.replace_all(&text, " ");
    let mut words: Vec<&str> = text.split(crate::text::py_isspace).filter(|w| !w.is_empty()).collect();
    while words.last().is_some_and(|w| edge(w).is_empty() || edge(w) == GAP) {
        let gap = words.pop().is_some_and(|w| edge(w) == GAP);
        while words.last().is_some_and(|w| edge(w).is_empty()) {
            words.pop();
        }
        if gap && words.last().is_some_and(|w| DANGLING.contains(&edge(w).to_lowercase().as_str())) {
            words.pop();
        }
    }
    while words.first().is_some_and(|w| edge(w).is_empty() || edge(w) == GAP) {
        words.remove(0);
    }
    // What is left of a gap in the middle as mere punctuation separates nothing.
    if words.iter().any(|w| edge(w) == GAP) {
        words.retain(|w| !edge(w).is_empty() && edge(w) != GAP);
    }
    words.join(" ").trim_matches(EDGE).to_string()
}

/// The words all candidates begin with, at least two (`_common_start`).
fn common_start(candidates: &BTreeSet<String>) -> String {
    let split: Vec<Vec<&str>> = candidates.iter().map(|c| c.split_whitespace().collect()).collect();
    let shortest = split.iter().map(Vec::len).min().unwrap_or(0);
    let mut common: Vec<&str> = Vec::new();
    for i in 0..shortest {
        let first = split[0][i];
        if split.iter().any(|words| words[i].to_lowercase() != first.to_lowercase()) {
            break;
        }
        common.push(first);
    }
    if common.len() < 2 {
        return String::new();
    }
    clean(&common.join(" "))
}

/// Python's `str.splitlines()`.
fn split_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let breaks = matches!(
            c,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        );
        if !breaks {
            continue;
        }
        out.push(&text[start..i]);
        let mut end = i + c.len_utf8();
        if c == '\r' && chars.peek().is_some_and(|&(_, n)| n == '\n') {
            chars.next();
            end += 1;
        }
        start = end;
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The title lines of a `global.ini`: key → text; `,P` behind a key is a
/// text variant and not part of it (`titles_from_ini`).
pub fn titles_from_ini(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for line in split_lines(text) {
        let Some((key, value)) = line.split_once('=') else { continue };
        if !key.to_lowercase().contains("title") {
            continue;
        }
        let key = py_strip(key.split(',').next().unwrap_or(key));
        let value = py_strip(value);
        if !key.is_empty() && !value.is_empty() && seen.insert(key.to_string()) {
            out.push((key.to_string(), value.to_string()));
        }
    }
    out
}

/// The words of a text, in lower case (`words_of`).
pub fn words_of(text: &str) -> Vec<String> {
    WORD.find_iter(text).map(|m| m.as_str().to_lowercase()).collect()
}

/// The English titles and words (`titel`, `woerter` of the stored file).
#[derive(Clone, Debug, Default)]
pub struct Originals {
    /// Key → English title, in the order of the file.
    pub titles: Vec<(String, String)>,
    pub vocabulary: HashSet<String>,
}

impl Originals {
    /// The stored file of the data folder, as it is; empty without one.
    pub fn load(paths: &Paths) -> Originals {
        jsonio::read_json(&paths.app_file(TITLES_FILE)).map(|d| Originals::of(&d)).unwrap_or_default()
    }

    /// From a parsed `titel-original.json`; both parts must be there.
    pub fn of(data: &Value) -> Originals {
        let (Some(Value::Object(titles)), Some(Value::Array(words))) = (data.get("titel"), data.get("woerter")) else {
            return Originals::default();
        };
        Originals {
            titles: titles.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect(),
            vocabulary: words.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        }
    }

    /// The titles written as a `global.ini` again, so a player without a
    /// translation finds his title too (`originals_as_ini`).
    pub fn as_ini(&self) -> String {
        self.titles.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("\n")
    }
}

/// A pattern for a title with placeholders: the pattern with groups, its
/// placeholder names, weak and weight for the ranking.
struct TitlePattern {
    rx: Regex,
    key: String,
    names: Vec<String>,
    weak: bool,
    weight: usize,
}

/// Displayed title → keys, from the texts of the language files
/// (`build_index`).
pub struct TitleIndex {
    exact: HashMap<String, BTreeSet<String>>,
    patterns: Vec<TitlePattern>,
}

impl TitleIndex {
    pub fn build(texts: &[String], known: &HashSet<&str>) -> TitleIndex {
        let mut exact: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut patterns = Vec::new();
        for text in texts {
            for (key, value) in titles_from_ini(text) {
                if !known.contains(key.as_str()) {
                    continue;
                }
                let plain = squeeze(&mission_log::clean(&value));
                if plain.is_empty() {
                    continue;
                }
                if plain.contains("~mission(") {
                    // ⚠ A template without a word of its own fits any title.
                    if words_of(&PLACEHOLDER.replace_all(&plain, " ")).is_empty() {
                        continue;
                    }
                    let parts = pieces(&plain);
                    let raw = parts.iter().map(|p| regex::escape(p)).collect::<Vec<_>>().join("(.+)");
                    if let Ok(rx) = Regex::new(&format!("(?i)^{raw}$")) {
                        patterns.push(TitlePattern {
                            rx,
                            key,
                            names: PLACEHOLDER_NAME.captures_iter(&plain).map(|c| c[1].to_string()).collect(),
                            weak: weak(&parts),
                            weight: weight(&parts),
                        });
                    }
                } else {
                    exact.entry(plain.to_lowercase()).or_default().insert(key);
                }
            }
        }
        TitleIndex { exact, patterns }
    }

    /// What fits a displayed title: key → placeholder values; only the most
    /// exact patterns count (`matches_for`).
    pub fn matches_for(&self, title: &str) -> BTreeMap<String, HashMap<String, String>> {
        let plain = squeeze(&mission_log::clean(title));
        if plain.is_empty() {
            return BTreeMap::new();
        }
        if let Some(hit) = self.exact.get(&plain.to_lowercase()) {
            return hit.iter().map(|k| (k.clone(), HashMap::new())).collect();
        }
        let matching: Vec<(&TitlePattern, HashMap<String, String>)> = self
            .patterns
            .iter()
            .filter_map(|p| {
                let caps = p.rx.captures(&plain)?;
                let values = p
                    .names
                    .iter()
                    .zip(caps.iter().skip(1))
                    .map(|(n, c)| (n.clone(), c.map_or("", |m| m.as_str()).to_string()));
                Some((p, values.collect()))
            })
            .collect();
        let Some(rank) = matching.iter().map(|(p, _)| (!p.weak, p.weight)).max() else { return BTreeMap::new() };
        let mut out = BTreeMap::new();
        for (p, values) in matching {
            if (!p.weak, p.weight) == rank {
                out.insert(p.key.clone(), values);
            }
        }
        out
    }
}

/// The English title filled with the game's values where that helps: a
/// value stays only when it is all English words and no person (`fill`).
pub fn fill(template: &str, values: &HashMap<String, String>, vocabulary: &HashSet<String>) -> String {
    PLACEHOLDER_NAME
        .replace_all(template, |caps: &regex::Captures| {
            let name = &caps[1];
            let value = py_strip(values.get(name).map_or("", String::as_str));
            let words = words_of(value);
            if !value.is_empty()
                && !words.is_empty()
                && !PERSON.is_match(name)
                && words.iter().all(|w| vocabulary.contains(w))
            {
                value.to_string()
            } else {
                caps[0].to_string()
            }
        })
        .into_owned()
}

/// A foreign title without an entry in the language files: the English words
/// in it lead to the original when exactly one English title holds them all
/// (`guess`). Empty when the title is English already or nothing fits.
pub fn guess(title: &str, originals: &Originals) -> String {
    let vocabulary = &originals.vocabulary;
    let plain = clean(title);
    let shown: Vec<&str> = WORD.find_iter(&plain).map(|m| m.as_str()).collect();
    if shown.is_empty() || shown.iter().all(|w| vocabulary.contains(&w.to_lowercase())) {
        return String::new();
    }
    let shown_lower: HashSet<String> = shown.iter().map(|w| w.to_lowercase()).collect();
    let significant: HashSet<String> = shown
        .iter()
        .map(|w| (w.to_lowercase(), w.chars().count()))
        .filter(|(l, n)| vocabulary.contains(l) && !DANGLING.contains(&l.as_str()) && *n > 2)
        .map(|(l, _)| l)
        .collect();
    if significant.len() >= MIN_GUESS_WORDS {
        let mut candidates: BTreeSet<String> = originals
            .titles
            .iter()
            .filter(|(_, v)| {
                let own: HashSet<String> = words_of(v).into_iter().collect();
                significant.is_subset(&own)
            })
            .map(|(_, v)| clean(v))
            .collect();
        candidates.remove("");
        if candidates.len() == 1 {
            return candidates.pop_first().unwrap_or_default();
        }
        if candidates.len() > 1 {
            let common = common_start(&candidates);
            if !common.is_empty() {
                return common;
            }
        }
    }
    let mut best: BTreeSet<String> = BTreeSet::new();
    let mut best_size = 0;
    for (_, value) in &originals.titles {
        let own: HashSet<String> = words_of(&PLACEHOLDER.replace_all(value, " "))
            .into_iter()
            .filter(|w| !DANGLING.contains(&w.as_str()))
            .collect();
        if own.len() < MIN_GUESS_WORDS || !own.is_subset(&shown_lower) {
            continue;
        }
        if own.len() > best_size {
            best = BTreeSet::from([clean(value)]);
            best_size = own.len();
        } else if own.len() == best_size {
            best.insert(clean(value));
        }
    }
    best.remove("");
    if best.len() == 1 {
        return best.pop_first().unwrap_or_default();
    }
    if best.is_empty() { String::new() } else { common_start(&best) }
}

/// Originals and the index over the game folder, built once.
pub struct Search {
    pub originals: Originals,
    index: TitleIndex,
    english: HashMap<String, String>,
}

impl Search {
    /// The `global.ini` texts of the installation plus the originals as INI.
    pub fn new(originals: Originals, texts: Vec<String>) -> Search {
        let mut texts = texts;
        texts.push(originals.as_ini());
        let known: HashSet<&str> = originals.titles.iter().map(|(k, _)| k.as_str()).collect();
        let index = TitleIndex::build(&texts, &known);
        let english = originals.titles.iter().cloned().collect();
        Search { originals, index, english }
    }

    /// From the data folder and the game folder.
    pub fn load(paths: &Paths) -> Search {
        Search::with_ini(paths, &mission_log::ini_files(paths))
    }

    /// The same with the given language files instead of the installation's.
    pub fn with_ini(paths: &Paths, ini_files: &[PathBuf]) -> Search {
        let originals = Originals::load(paths);
        if originals.titles.is_empty() {
            return Search::new(originals, Vec::new());
        }
        Search::new(originals, local_texts(ini_files))
    }

    /// The search term for a contract: English when it can be found
    /// (`search_text`). Several keys with different English titles give their
    /// common start, else the displayed title: rather search loosely than for
    /// another contract.
    pub fn search_text(&self, title: &str) -> String {
        let fallback = clean(title);
        if self.originals.titles.is_empty() {
            return fallback;
        }
        let found = self.index.matches_for(title);
        let mut english: BTreeSet<String> = found
            .iter()
            .filter_map(|(k, values)| self.english.get(k).map(|t| clean(&fill(t, values, &self.originals.vocabulary))))
            .collect();
        english.remove("");
        if english.len() == 1 {
            return english.pop_first().unwrap_or_default();
        }
        if english.len() > 1 {
            let common = common_start(&english);
            return if common.is_empty() { fallback } else { common };
        }
        let guessed = guess(title, &self.originals);
        if guessed.is_empty() { fallback } else { guessed }
    }

    /// The address of the Issue Council search for this contract (`url_for`).
    pub fn url_for(&self, title: &str) -> String {
        url_with(&self.search_text(title))
    }
}

/// The contents of the language files, as Python reads them (`errors='ignore'`).
pub fn local_texts(files: &[PathBuf]) -> Vec<String> {
    files.iter().filter_map(|p| std::fs::read(p).ok()).map(|b| decode_ignore(&b)).collect()
}

/// Python's `urllib.parse.quote(text, safe='')`.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The search address for a ready search term.
pub fn url_with(search: &str) -> String {
    format!("{BASE_URL}?search={}&sort=relevance&statuses=open", quote(search))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn originals(pairs: &[(&str, &str)], words: &[&str]) -> Originals {
        Originals {
            titles: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            vocabulary: words.iter().map(|w| w.to_string()).collect(),
        }
    }

    #[test]
    fn clean_drops_marks_placeholders_and_dangling_words() {
        assert_eq!(clean("Help Headhunters at ~mission(Location)"), "Help Headhunters");
        assert_eq!(
            clean("<EM4>[10 Rep] [BP]*</EM4> Pro Tem Bounty Assignment: ~mission(TargetName)"),
            "Pro Tem Bounty Assignment"
        );
        // ⚠ The gap inside the parentheses keeps them from counting as empty.
        assert_eq!(clean("Verified Bounty: ~mission(A) | ~mission(B) (~mission(C))"), "Verified Bounty: ( )");
        // Words that end a title by themselves stay; a dash without a gap stays.
        assert_eq!(clean("Crew Hasn't Checked In"), "Crew Hasn't Checked In");
        assert_eq!(clean("Combat Gauntlet - Scenario #5"), "Combat Gauntlet - Scenario #5");
        assert_eq!(clean("[800 Rep] Escort"), "Escort");
    }

    #[test]
    fn the_english_title_with_english_values_only() {
        let o = originals(
            &[
                ("Defend_Title", "Protect ~mission(Item) and Escort"),
                ("Help_Title", "Help Headhunters at ~mission(Location)"),
            ],
            &["protect", "fuel", "tanks", "and", "escort", "help", "headhunters", "at"],
        );
        let texts = vec![
            "Defend_Title=Schütze ~mission(Item) und Begleite\nHelp_Title=Hilf Kopfgeldjägern bei ~mission(Location)"
                .to_string(),
        ];
        let search = Search::new(o, texts);
        assert_eq!(search.search_text("Schütze Fuel Tanks und Begleite"), "Protect Fuel Tanks and Escort");
        assert_eq!(search.search_text("Hilf Kopfgeldjägern bei Asteroiden Bergbaubasis"), "Help Headhunters");
        // English already and unknown: the cleaned title.
        assert_eq!(search.search_text("Unknown Thing <EM4>[BP]</EM4>"), "Unknown Thing");
    }

    #[test]
    fn a_guess_needs_one_original_holding_all_english_words() {
        let o = originals(
            &[("A", "Emergency: Blinding Hope in Trouble"), ("B", "Deliver Cargo")],
            &["emergency", "blinding", "hope", "in", "trouble", "deliver", "cargo"],
        );
        assert_eq!(guess("Notfall: Blinding Hope in Schwierigkeiten", &o), "Emergency: Blinding Hope in Trouble");
        assert_eq!(guess("Deliver Cargo", &o), "");
    }

    #[test]
    fn the_address_is_quoted_like_python() {
        assert_eq!(quote("Help Headhunters: ä/&"), "Help%20Headhunters%3A%20%C3%A4%2F%26");
        assert_eq!(
            url_with("Retake Platforms"),
            "https://issue-council.robertsspaceindustries.com/projects/STAR-CITIZEN/issues?search=Retake%20Platforms&sort=relevance&statuses=open"
        );
    }

    #[test]
    fn ini_titles_take_the_first_and_drop_the_variant() {
        let found = titles_from_ini("x_Title,P = Eins \r\nx_Title=Zwei\nnoTitle\ny_desc=Nein\u{2028}z_title=Drei");
        assert_eq!(
            found,
            vec![("x_Title".to_string(), "Eins".to_string()), ("z_title".to_string(), "Drei".to_string())]
        );
    }
}
