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

//! The look of a memo's text: bold, italic, underlined and struck-through
//! letters, and lines that stand in a bulleted or a numbered list.
//!
//! The text itself stays plain. The look is kept beside it in the memo's
//! entry under `formatierung`:
//! `{"laenge": letters, "fett": [[from, to], …], "kursiv": …,
//! "unterstrichen": …, "durchgestrichen": …, "punkte": [line, …],
//! "nummern": [line, …], "tabelle": [line, …]}`. Places count letters (Unicode scalar values),
//! lines from 0; a range ends before `to`. A memo without any look has no
//! such entry, so its file reads exactly as before.
//!
//! ⚠ A program that knows nothing of the look reads the plain text and drops
//! the look when it writes the file; the text is never touched. A look whose
//! `laenge` does not match the text (the text was changed elsewhere) is
//! dropped on reading, so that no letter is shown in a look it never had.

use std::ops::Range;

use crate::jsonio::{Map, Value};

/// Bold letters.
pub const BOLD: u8 = 1;
/// Italic letters.
pub const ITALIC: u8 = 2;
/// Underlined letters.
pub const UNDERLINE: u8 = 4;
/// Struck-through letters.
pub const STRIKE: u8 = 8;

/// The styles of letters with their key in the file.
const STYLE_KEYS: [(u8, &str); 4] =
    [(BOLD, "fett"), (ITALIC, "kursiv"), (UNDERLINE, "unterstrichen"), (STRIKE, "durchgestrichen")];
/// The key of the length check.
const LENGTH_KEY: &str = "laenge";
/// The kinds of lines with their key in the file.
const LINE_KEYS: [(LineKind, &str); 3] =
    [(LineKind::Bullet, "punkte"), (LineKind::Number, "nummern"), (LineKind::Table, "tabelle")];

/// How a line stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineKind {
    #[default]
    Plain,
    /// A point of a bulleted list.
    Bullet,
    /// A point of a numbered list.
    Number,
    /// A row of a table; its cells are parted by tabs (see [`super::table`]).
    Table,
}

/// The look of a text. Letters and lines beyond what is stored are plain, so the empty look is the plain text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Formatting {
    /// The styles of each letter by its place (a set of [`BOLD`], [`ITALIC`], [`UNDERLINE`], [`STRIKE`]).
    letters: Vec<u8>,
    /// The kind of each line by its number.
    lines: Vec<LineKind>,
}

fn letter_count(text: &str) -> usize {
    text.chars().count()
}

fn line_count(text: &str) -> usize {
    text.matches('\n').count() + 1
}

fn newlines(chars: &[char]) -> usize {
    chars.iter().filter(|c| **c == '\n').count()
}

impl Formatting {
    /// True for a look with nothing in it.
    pub fn is_plain(&self) -> bool {
        self.letters.is_empty() && self.lines.is_empty()
    }

    /// Drops the plain tail, so that the same look always compares equal.
    fn trim(&mut self) {
        while self.letters.last() == Some(&0) {
            self.letters.pop();
        }
        while self.lines.last() == Some(&LineKind::Plain) {
            self.lines.pop();
        }
    }

    /// The styles of the letter at `at`.
    pub fn style_at(&self, at: usize) -> u8 {
        self.letters.get(at).copied().unwrap_or(0)
    }

    /// The styles a letter typed at `at` gets: those of the letter before it, or of the first letter where
    /// there is none before.
    pub fn style_before(&self, at: usize) -> u8 {
        self.style_at(at.checked_sub(1).unwrap_or(at))
    }

    /// The kind of the line `line`.
    pub fn line_kind(&self, line: usize) -> LineKind {
        self.lines.get(line).copied().unwrap_or_default()
    }

    /// Whether every letter in `range` has `style`; false for an empty range.
    pub fn all_have(&self, range: Range<usize>, style: u8) -> bool {
        !range.is_empty() && range.into_iter().all(|at| self.style_at(at) & style != 0)
    }

    /// Gives every letter in `range` the `style`, or takes it away.
    pub fn set_style(&mut self, range: Range<usize>, style: u8, on: bool) {
        if range.is_empty() {
            return;
        }
        if self.letters.len() < range.end {
            self.letters.resize(range.end, 0);
        }
        for letter in &mut self.letters[range] {
            if on {
                *letter |= style;
            } else {
                *letter &= !style;
            }
        }
        self.trim();
    }

    /// Whether every line in `lines` is of `kind`.
    pub fn all_lines(&self, lines: Range<usize>, kind: LineKind) -> bool {
        lines.into_iter().all(|line| self.line_kind(line) == kind)
    }

    /// Sets the kind of the lines in `lines`.
    pub fn set_lines(&mut self, lines: Range<usize>, kind: LineKind) {
        if lines.is_empty() {
            return;
        }
        if self.lines.len() < lines.end {
            self.lines.resize(lines.end, LineKind::Plain);
        }
        for line in &mut self.lines[lines] {
            *line = kind;
        }
        self.trim();
    }

    /// The number a line of a numbered list shows: its place in the run of numbered lines it stands in,
    /// counted from 1; `None` for any other line.
    pub fn number_of(&self, line: usize) -> Option<usize> {
        if self.line_kind(line) != LineKind::Number {
            return None;
        }
        let before = (0..line).rev().take_while(|l| self.line_kind(*l) == LineKind::Number).count();
        Some(before + 1)
    }

    /// The pieces of letters that share their styles, as (letters, styles), through the end of `count`
    /// letters.
    pub fn runs(&self, count: usize) -> Vec<(Range<usize>, u8)> {
        let mut runs: Vec<(Range<usize>, u8)> = Vec::new();
        for at in 0..count {
            let style = self.style_at(at);
            match runs.last_mut() {
                Some((range, last)) if *last == style => range.end = at + 1,
                _ => runs.push((at..at + 1, style)),
            }
        }
        runs
    }

    /// Follows a change of the text from `old` to `new`, so that each letter keeps its look and each line
    /// its kind. `cursor` is where the cursor stands after the change (in letters), which tells where an
    /// insertion went where the text alone leaves it open ("aa" to "aaa"). New letters get `typing`, or,
    /// without it, the styles of the letter before them (the first letter's where there is none before).
    /// New lines are of the kind of the line they were split from, so that a list goes on.
    pub fn follow_edit(&mut self, old: &str, new: &str, cursor: Option<usize>, typing: Option<u8>) {
        let old: Vec<char> = old.chars().collect();
        let new: Vec<char> = new.chars().collect();
        let shorter = old.len().min(new.len());
        let same_start = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let same_end = old.iter().rev().zip(new.iter().rev()).take_while(|(a, b)| a == b).count();
        // The change ends where the cursor stands; without one it starts as late as the text allows.
        let (start, same_end) = match cursor {
            Some(cursor) => {
                let same_end = same_end.min(new.len().saturating_sub(cursor)).min(shorter);
                (same_start.min(shorter - same_end), same_end)
            }
            None => (same_start, same_end.min(shorter - same_start)),
        };
        let (old_end, new_end) = (old.len() - same_end, new.len() - same_end);
        let style = typing.unwrap_or_else(|| self.style_before(start));
        self.splice(&old, start..old_end, &new[start..new_end], style);
    }

    /// Replaces the letters `gone` of `old` by `came` in the look: the new letters get `style`, new lines the
    /// kind of the line they were split from.
    fn splice(&mut self, old: &[char], gone: Range<usize>, came: &[char], style: u8) {
        self.letters.resize(self.letters.len().max(old.len()), 0);
        self.letters.truncate(old.len());
        self.letters.splice(gone.clone(), std::iter::repeat_n(style, came.len()));

        let line = newlines(&old[..gone.start]);
        let kind = self.line_kind(line);
        let lines_gone = newlines(&old[gone]);
        self.lines.resize(self.lines.len().max(newlines(old) + 1), LineKind::Plain);
        self.lines.truncate(newlines(old) + 1);
        self.lines.splice(line + 1..line + 1 + lines_gone, std::iter::repeat_n(kind, newlines(came)));
        self.trim();
    }

    /// Replaces the letters `gone` of `text` by `came`, in the text and in its look together; the new letters
    /// are plain.
    pub fn replace(&mut self, text: &mut String, gone: Range<usize>, came: &str) {
        let old: Vec<char> = text.chars().collect();
        let byte = |at: usize| text.char_indices().nth(at).map_or(text.len(), |(byte, _)| byte);
        let (from, to) = (byte(gone.start), byte(gone.end));
        self.splice(&old, gone, &came.chars().collect::<Vec<_>>(), 0);
        text.replace_range(from..to, came);
    }

    /// The line a change from `old` to `new` leaves, where it is the one key that ends a list: a line break
    /// typed into an empty point of a list (as in a word processor, it ends the list there instead of
    /// adding another empty point). `cursor` is where the cursor stands after it (in letters), which tells
    /// which of several line breaks in a row was typed.
    pub fn break_ends_list(&self, old: &str, new: &str, cursor: Option<usize>) -> Option<usize> {
        if new.len() != old.len() + 1 {
            return None;
        }
        // Every place where the one line break could have gone in.
        let first = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
        let mut places: Vec<usize> = Vec::new();
        let mut at = first;
        loop {
            if new.as_bytes().get(at) == Some(&b'\n') && new[at + 1..] == old[at..] && new[..at] == old[..at] {
                places.push(at);
            }
            if at == 0 || old.as_bytes()[at - 1] != b'\n' {
                break;
            }
            at -= 1;
        }
        if let Some(cursor) = cursor {
            let typed = new.char_indices().nth(cursor.checked_sub(1)?).map(|(byte, _)| byte)?;
            places.retain(|at| *at == typed);
        }
        places.into_iter().find_map(|at| {
            let empty = (at == 0 || old.as_bytes()[at - 1] == b'\n') && old.as_bytes().get(at).is_none_or(|b| *b == b'\n');
            let line = old[..at].matches('\n').count();
            let listed = matches!(self.line_kind(line), LineKind::Bullet | LineKind::Number);
            (empty && listed).then_some(line)
        })
    }

    /// The look as it is written into the memo's entry; `None` for the plain text, which writes nothing.
    pub fn to_value(&self, text: &str) -> Option<Value> {
        let mut look = self.clone();
        look.letters.truncate(letter_count(text));
        look.lines.truncate(line_count(text));
        look.trim();
        if look.is_plain() {
            return None;
        }
        let mut map: Map = vec![(LENGTH_KEY.into(), Value::int(letter_count(text) as i64))];
        for (style, key) in STYLE_KEYS {
            let ranges: Vec<Value> = look
                .runs(look.letters.len())
                .into_iter()
                .filter(|(_, styles)| styles & style != 0)
                .fold(Vec::<Range<usize>>::new(), |mut merged, (range, _)| {
                    match merged.last_mut() {
                        Some(last) if last.end == range.start => last.end = range.end,
                        _ => merged.push(range),
                    }
                    merged
                })
                .into_iter()
                .map(|r| Value::Array(vec![Value::int(r.start as i64), Value::int(r.end as i64)]))
                .collect();
            if !ranges.is_empty() {
                map.push((key.into(), Value::Array(ranges)));
            }
        }
        for (kind, key) in LINE_KEYS {
            let lines: Vec<Value> = (0..look.lines.len())
                .filter(|line| look.line_kind(*line) == kind)
                .map(|line| Value::int(line as i64))
                .collect();
            if !lines.is_empty() {
                map.push((key.into(), Value::Array(lines)));
            }
        }
        Some(Value::Object(map))
    }

    /// The look stored for `text`; plain where there is none, where it is broken, or where its length does
    /// not match the text. Ranges and lines beyond the text are left out.
    pub fn from_value(value: Option<&Value>, text: &str) -> Formatting {
        let Some(Value::Object(map)) = value else { return Formatting::default() };
        let count = letter_count(text);
        if field(map, LENGTH_KEY).and_then(whole) != Some(count) {
            return Formatting::default();
        }
        let mut look = Formatting::default();
        for (style, key) in STYLE_KEYS {
            for pair in items(map, key) {
                let Value::Array(pair) = pair else { continue };
                let (Some(from), Some(to)) = (pair.first().and_then(whole), pair.get(1).and_then(whole)) else {
                    continue;
                };
                look.set_style(from.min(count)..to.min(count), style, true);
            }
        }
        let lines = line_count(text);
        for (kind, key) in LINE_KEYS {
            for line in items(map, key).iter().filter_map(whole).filter(|line| *line < lines) {
                look.set_lines(line..line + 1, kind);
            }
        }
        look
    }
}

/// The number of the line the letter at `at` stands on (a place past the end counts to the last line).
pub fn line_of(text: &str, at: usize) -> usize {
    text.chars().take(at).filter(|c| *c == '\n').count()
}

/// Whether the place `at` (in letters) is the start of a line.
pub fn starts_line(text: &str, at: usize) -> bool {
    at == 0 || text.chars().nth(at - 1) == Some('\n')
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn items<'a>(map: &'a Map, key: &str) -> &'a [Value] {
    match field(map, key) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

/// A whole number of zero or more.
fn whole(value: &Value) -> Option<usize> {
    match value {
        Value::Int(n) => n.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold_letters(look: &Formatting, text: &str) -> String {
        text.chars().enumerate().filter(|(at, _)| look.style_at(*at) & BOLD != 0).map(|(_, c)| c).collect()
    }

    #[test]
    fn typing_inside_bold_letters_types_bold_and_after_them_too() {
        let mut look = Formatting::default();
        look.set_style(0..4, BOLD, true);
        look.follow_edit("fett rest", "fextt rest", Some(3), None);
        assert_eq!(bold_letters(&look, "fextt rest"), "fextt");
        look.follow_edit("fextt rest", "fexttX rest", Some(6), None);
        assert_eq!(bold_letters(&look, "fexttX rest"), "fexttX", "a letter after bold ones is bold as well");
        look.follow_edit("fexttX rest", "fexttX restY", Some(12), None);
        assert_eq!(bold_letters(&look, "fexttX restY"), "fexttX");
    }

    #[test]
    fn the_cursor_tells_where_a_letter_went_among_equal_ones() {
        // "aa" with the first a bold: an a typed at the front is bold, not the last one.
        let mut look = Formatting::default();
        look.set_style(0..1, BOLD, true);
        look.follow_edit("aa", "aaa", Some(1), None);
        assert_eq!((0..3).map(|at| look.style_at(at)).collect::<Vec<_>>(), [BOLD, BOLD, 0]);
        let mut look = Formatting::default();
        look.set_style(0..1, BOLD, true);
        look.follow_edit("aa", "aaa", Some(3), None);
        assert_eq!((0..3).map(|at| look.style_at(at)).collect::<Vec<_>>(), [BOLD, 0, 0]);
    }

    #[test]
    fn deleting_takes_the_look_of_the_deleted_letters_with_it() {
        let mut look = Formatting::default();
        look.set_style(2..5, ITALIC, true);
        look.follow_edit("abcdefg", "abfg", Some(2), None);
        assert_eq!((0..4).map(|at| look.style_at(at)).collect::<Vec<_>>(), [0, 0, 0, 0]);
        let mut look = Formatting::default();
        look.set_style(4..6, ITALIC, true);
        look.follow_edit("abcdefg", "aefg", Some(1), None);
        assert_eq!((0..4).map(|at| look.style_at(at)).collect::<Vec<_>>(), [0, ITALIC, ITALIC, 0]);
    }

    #[test]
    fn a_style_chosen_before_typing_is_the_one_typed() {
        let mut look = Formatting::default();
        look.follow_edit("ab", "abc", Some(3), Some(BOLD | UNDERLINE));
        assert_eq!(look.style_at(2), BOLD | UNDERLINE);
        assert_eq!(look.style_at(1), 0);
    }

    #[test]
    fn a_new_line_in_a_list_is_a_point_of_it_and_a_deleted_line_takes_its_kind_away() {
        let mut look = Formatting::default();
        look.set_lines(1..2, LineKind::Bullet);
        let old = "Titel\nerster Punkt\nText";
        let new = "Titel\nerster Punkt\nzweiter\nText";
        // Enter at the end of the point and a word typed: the cursor stands after the word.
        look.follow_edit(old, new, Some(26), None);
        assert_eq!((0..4).map(|l| look.line_kind(l)).collect::<Vec<_>>(), [
            LineKind::Plain,
            LineKind::Bullet,
            LineKind::Bullet,
            LineKind::Plain
        ]);
        // Joining the two points leaves one.
        look.follow_edit(new, "Titel\nerster Punktzweiter\nText", Some(18), None);
        assert_eq!((0..3).map(|l| look.line_kind(l)).collect::<Vec<_>>(), [
            LineKind::Plain,
            LineKind::Bullet,
            LineKind::Plain
        ]);
    }

    #[test]
    fn numbers_count_within_each_run_of_numbered_lines() {
        let mut look = Formatting::default();
        look.set_lines(1..4, LineKind::Number);
        look.set_lines(5..7, LineKind::Number);
        let numbers: Vec<_> = (0..8).map(|l| look.number_of(l)).collect();
        assert_eq!(numbers, [None, Some(1), Some(2), Some(3), None, Some(1), Some(2), None]);
    }

    #[test]
    fn a_break_in_an_empty_point_ends_the_list_and_elsewhere_does_not() {
        let mut look = Formatting::default();
        look.set_lines(1..3, LineKind::Bullet);
        let old = "Titel\neins\n\nText";
        let new = "Titel\neins\n\n\nText";
        assert_eq!(look.break_ends_list(old, new, None), Some(2));
        assert_eq!(look.break_ends_list(old, new, Some(12)), Some(2), "typed into the empty point");
        assert_eq!(look.break_ends_list(old, new, Some(11)), None, "typed after the full point: a new point");
        assert_eq!(look.break_ends_list(old, "Titel\nei\nns\n\nText", None), None, "a break inside a full point");
        assert_eq!(look.break_ends_list(old, "Titel\neins\n\nText\n", None), None, "a plain line");
        assert_eq!(look.break_ends_list(old, "Titel\neins\nx\nText", None), None, "a letter, no break");
    }

    #[test]
    fn the_look_is_written_as_ranges_and_lines_and_read_back_the_same() {
        let text = "Größe 🚀 prüfen\nPunkt\nnoch einer";
        let mut look = Formatting::default();
        look.set_style(0..5, BOLD, true);
        look.set_style(3..9, ITALIC, true);
        look.set_style(6..7, UNDERLINE | STRIKE, true);
        look.set_lines(1..3, LineKind::Bullet);
        let value = look.to_value(text).expect("a look is written");
        let written: String =
            crate::jsonio::dumps(&value, None, false).chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(
            written,
            r#"{"laenge":31,"fett":[[0,5]],"kursiv":[[3,9]],"unterstrichen":[[6,7]],"durchgestrichen":[[6,7]],"punkte":[1,2]}"#
        );
        assert_eq!(Formatting::from_value(Some(&value), text), look);
        assert_eq!(Formatting::default().to_value(text), None, "a plain text writes nothing");
    }

    #[test]
    fn a_look_that_does_not_fit_the_text_is_dropped_and_its_overhang_cut() {
        let mut look = Formatting::default();
        look.set_style(0..3, BOLD, true);
        let value = look.to_value("abc").unwrap();
        assert!(Formatting::from_value(Some(&value), "abcd").is_plain(), "the text was changed elsewhere");
        let wild: Value = crate::jsonio::loads(
            r#"{"laenge": 3, "fett": [[1, 99], "x", [2]], "kursiv": 4, "punkte": [0, 7, -1, "a"], "nummern": [1.5]}"#,
        )
        .unwrap();
        let read = Formatting::from_value(Some(&wild), "abc");
        assert_eq!(bold_letters(&read, "abc"), "bc");
        assert_eq!(read.line_kind(0), LineKind::Bullet);
        assert_eq!(read.lines.len(), 1, "no line beyond the text");
        assert!(Formatting::from_value(Some(&Value::str("fett")), "abc").is_plain());
    }
}
