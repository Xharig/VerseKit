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

//! The history of the versions for the page „Was ist neu“ (4.12 `updater.history`, `history_grouped`,
//! `points_by_kind`, `intro`).
//!
//! Two sources: the change logs built into the program (`CHANGELOG.md` German, `CHANGELOG.en.md` English) and the
//! release texts of GitHub. ⚠ The built-in log comes first, because only it knows the language; GitHub fills in
//! only the versions the log does not know yet (the ones newer than this program).

use std::collections::BTreeSet;
use std::sync::OnceLock;

use regex::Regex;

use crate::language::Lang;
use crate::text::{py_isspace, py_strip};
use crate::updater::{self, Release};
use crate::version::{is_prerelease, parts};

/// The change log in German.
const GERMAN: &str = include_str!("../../../CHANGELOG.md");
/// The change log in English.
const ENGLISH: &str = include_str!("../../../CHANGELOG.en.md");

/// The most entries the page shows.
pub const MOST_ENTRIES: usize = 25;

/// What a change is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    New,
    Better,
    Fix,
}

impl Kind {
    /// The kinds in the order of the filter on the page.
    pub const ALL: [Kind; 3] = [Kind::New, Kind::Better, Kind::Fix];
    /// The key of the word for the kind (and of the filter button).
    pub fn key(self) -> &'static str {
        match self {
            Kind::New => "s_wn_f_neu",
            Kind::Better => "s_wn_f_bess",
            Kind::Fix => "s_wn_f_fix",
        }
    }
}

/// Which heading means what. ⚠ Generous on purpose, German and English alike; whoever invents a heading ends
/// up under „new“ and not in nothing. The first kind with a word in the heading wins, in this order.
const KINDS: [(Kind, &[&str]); 3] = [
    (Kind::Fix, &["behoben", "fixed", "korrigiert", "bugfix"]),
    (Kind::Better, &["geändert", "changed", "verbessert", "improved", "entfernt", "removed", "bedienung"]),
    (Kind::New, &["hinzugefügt", "added", "neu", "new"]),
];

/// One version in the history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// `v4.12.0`; a series grouped into one is `v4.12`.
    pub version: String,
    /// `2026-10-19`, or empty.
    pub date: String,
    /// The Markdown text of the version.
    pub text: String,
    /// The built-in change log knew it (else GitHub).
    pub built_in: bool,
}

/// A line of text with universal newlines, as a file read in text mode.
fn universal(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The sentence that introduces a version: the `> …` block above the lists. ⚠ `points_by_kind` throws away
/// everything that is no list; this sentence belongs under the version when it is opened.
pub fn intro(text: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let stripped = py_strip(line);
        if stripped.starts_with('>') {
            lines.push(py_strip(stripped.trim_start_matches('>')));
        } else if !lines.is_empty() {
            break;
        }
    }
    let sentence = lines.iter().filter(|l| !l.is_empty()).copied().collect::<Vec<_>>().join(" ");
    py_strip(&sentence.replace("**", "").replace('`', "")).to_string()
}

/// The changes of a text as (kind, line), for the filter and the marks.
///
/// A heading (`### Behoben`) sets the kind for everything below it; one that no word fits keeps the kind before.
/// Without a heading it is „new“: rather sorted wrongly than invisible. ⚠ Fenced blocks are skipped (as a line
/// stuck to a point they were gibberish). ⚠ The indent is looked at before the line is stripped, or sub-points
/// could not be told from main points: an indented line without a bullet continues the point above.
pub fn points_by_kind(text: &str) -> Vec<(Kind, String)> {
    let mut kind = Kind::New;
    let mut out: Vec<(Kind, String)> = Vec::new();
    let mut in_block = false;
    for line in text.split('\n') {
        let stripped = py_strip(line);
        if stripped.starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if in_block {
            continue;
        }
        let indented = line.chars().next().is_some_and(py_isspace);
        if stripped.starts_with('#') {
            let lower = py_strip(stripped.trim_start_matches('#')).to_lowercase();
            if let Some((found, _)) = KINDS.iter().find(|(_, words)| words.iter().any(|w| lower.contains(w))) {
                kind = *found;
            }
            continue;
        }
        let bullet = stripped.starts_with("- ") || stripped.starts_with("* ");
        if !indented && bullet {
            out.push((kind, py_strip(&stripped[2..]).to_string()));
        } else if indented
            && !stripped.is_empty()
            && !bullet
            && let Some(last) = out.last_mut()
        {
            last.1 = py_strip(&format!("{} {}", last.1, stripped)).to_string();
        }
    }
    out
}

fn pattern(cell: &'static OnceLock<Regex>, source: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(source).unwrap_or_else(|_| unreachable!("a fixed pattern")))
}

/// A line without the Markdown marks that would show as signs.
pub fn clean_row(line: &str) -> String {
    static BOLD: OnceLock<Regex> = OnceLock::new();
    static CODE: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    let line = pattern(&BOLD, r"\*\*(.+?)\*\*").replace_all(line, "$1");
    let line = pattern(&CODE, r"`([^`]+)`").replace_all(&line, "$1");
    let line = pattern(&LINK, r"\[([^\]]+)\]\([^)]+\)").replace_all(&line, "$1");
    // ⚠ What is left of backticks goes too: the pairs above take only pairs.
    py_strip(&line.replace('`', "")).to_string()
}

/// The part of a bilingual release text that fits the language: English above, German in a `<details>` block
/// below. Without the block the text stays whole; a missing part is made up by the other one.
pub fn language_part(text: &str, lang: Lang) -> String {
    static DETAILS: OnceLock<Regex> = OnceLock::new();
    let Some(found) = pattern(&DETAILS, r"(?is)<details>\s*<summary>.*?</summary>(.*?)</details>").captures(text)
    else {
        return text.to_string();
    };
    let start = found.get(0).map_or(0, |m| m.start());
    let german = found.get(1).map_or("", |m| py_strip(m.as_str()));
    let english = py_strip(py_strip(&text[..start]).trim_end_matches('-'));
    let (wanted, other) = if lang == Lang::De { (german, english) } else { (english, german) };
    if wanted.is_empty() { other } else { wanted }.to_string()
}

/// The blocks `## <version> - <date>` of a change log. A block without a version number (the unreleased one) and a
/// version seen before are left out; ⚠ a version is told apart by its whole [`updater::version_key`], so `rc1`
/// and `rc13` are not the same.
pub fn parse(raw: &str) -> Vec<Entry> {
    static DATE: OnceLock<Regex> = OnceLock::new();
    let raw = universal(raw);
    let mut blocks: Vec<(String, Vec<&str>)> = Vec::new();
    for line in raw.split('\n') {
        if let Some(heading) = line.strip_prefix("## ") {
            blocks.push((heading.to_string(), Vec::new()));
        } else if let Some(block) = blocks.last_mut() {
            block.1.push(line);
        }
    }
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for (heading, rest) in blocks {
        let version = py_strip(heading.split('—').next().unwrap_or("").split(" - ").next().unwrap_or(""));
        if parts(version) == (0, 0, 0) || !seen.insert(updater::version_key(version)) {
            continue;
        }
        let date = pattern(&DATE, r"(\d{4}-\d{2}-\d{2})").find(&heading).map_or("", |m| m.as_str());
        entries.push(Entry {
            version: version.to_string(),
            date: date.to_string(),
            text: py_strip(&rest.join("\n")).to_string(),
            built_in: true,
        });
    }
    entries
}

/// The history, newest first: the log of the language first (the other one if it is empty), then what GitHub
/// knows beyond it. The GitHub text is cut to the language too.
pub fn history(lang: Lang, releases: &[Release]) -> Vec<Entry> {
    let (own, other) = if lang == Lang::De { (GERMAN, ENGLISH) } else { (ENGLISH, GERMAN) };
    history_of(if own.trim().is_empty() { other } else { own }, lang, releases)
}

/// [`history`] on the text of a change log.
pub fn history_of(raw: &str, lang: Lang, releases: &[Release]) -> Vec<Entry> {
    let mut entries = parse(raw);
    let mut seen: BTreeSet<_> = entries.iter().map(|e| updater::version_key(&e.version)).collect();
    for release in releases {
        if release.version.is_empty() || !seen.insert(updater::version_key(&release.version)) {
            continue;
        }
        entries.push(Entry {
            version: release.version.clone(),
            date: release.date.clone(),
            text: py_strip(&language_part(&release.text, lang)).to_string(),
            built_in: false,
        });
    }
    // ⚠ By the version, not by the number: `rc1` to `rc13` all carry the same date. Equal ones keep their place.
    entries
        .sort_by(|a, b| (updater::version_key(&b.version), &b.date).cmp(&(updater::version_key(&a.version), &a.date)));
    entries
}

/// The history with the patch versions of a series in one entry (`v3.13.0` to `.3` become `v3.13`).
///
/// ⚠ Test versions stay single (who runs one sees what that one brought) and vanish once the finished version
/// of their number is there. The lead and the date are the newest version's; the points of all stand below.
pub fn grouped(lang: Lang, releases: &[Release]) -> Vec<Entry> {
    group(history(lang, releases))
}

/// [`grouped`] on a history that is already made.
pub fn group(all: Vec<Entry>) -> Vec<Entry> {
    let finished: BTreeSet<_> = all.iter().filter(|e| !is_prerelease(&e.version)).map(|e| parts(&e.version)).collect();
    let mut out: Vec<Entry> = Vec::new();
    let mut by_series: Vec<((u64, u64), usize)> = Vec::new();
    for entry in all {
        if is_prerelease(&entry.version) {
            if !finished.contains(&parts(&entry.version)) {
                out.push(entry);
            }
            continue;
        }
        let (major, minor, _) = parts(&entry.version);
        match by_series.iter().find(|(series, _)| *series == (major, minor)) {
            Some((_, at)) => {
                let present = &mut out[*at];
                present.text = format!("{}\n\n{}", present.text, entry.text);
            }
            None => {
                by_series.push(((major, minor), out.len()));
                out.push(Entry { version: format!("v{major}.{minor}"), ..entry });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
