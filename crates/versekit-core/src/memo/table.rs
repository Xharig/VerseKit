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

//! Small tables in a memo: lines in a row marked [`LineKind::Table`] form a
//! table, one line per row, the cells of a row parted by a tab. In the plain
//! text (what a version without tables shows) a table reads as lines of
//! tab-separated words.
//!
//! Each change here edits the text and its look together and gives back
//! where the cursor goes (in letters). A row may hold fewer cells than the
//! widest one; the missing cells count as empty.

use std::ops::Range;

use super::format::{Formatting, LineKind, line_of};

/// One line of a text: its number, where its letters start and how many it has.
#[derive(Clone, Copy, Debug)]
struct Line {
    number: usize,
    start: usize,
    len: usize,
}

fn lines(text: &str) -> Vec<Line> {
    let mut found = Vec::new();
    let mut start = 0;
    for (number, line) in text.split('\n').enumerate() {
        let len = line.chars().count();
        found.push(Line { number, start, len });
        start += len + 1;
    }
    found
}

fn line_text(text: &str, line: Line) -> String {
    text.chars().skip(line.start).take(line.len).collect()
}

/// The lines of the table the line `line` stands in; `None` where it stands in none.
pub fn rows_around(look: &Formatting, text: &str, line: usize) -> Option<Range<usize>> {
    if look.line_kind(line) != LineKind::Table {
        return None;
    }
    let count = text.split('\n').count();
    let first = (0..line).rev().take_while(|l| look.line_kind(*l) == LineKind::Table).last().unwrap_or(line);
    let end = (line..count).take_while(|l| look.line_kind(*l) == LineKind::Table).last().unwrap_or(line) + 1;
    Some(first..end)
}

/// Whether the place `at` stands in a table.
pub fn in_table(look: &Formatting, text: &str, at: usize) -> bool {
    look.line_kind(line_of(text, at)) == LineKind::Table
}

/// The number of cells of the widest row of `rows`.
fn columns(text: &str, all: &[Line], rows: &Range<usize>) -> usize {
    rows.clone().map(|row| line_text(text, all[row]).split('\t').count()).max().unwrap_or(1)
}

/// The cell the place `at` stands in on its row (counted from 0).
fn cell_at(text: &str, line: Line, at: usize) -> usize {
    text.chars().skip(line.start).take(at.saturating_sub(line.start)).filter(|c| *c == '\t').count()
}

/// Where the cell `cell` of the row `line` starts (in letters); the end of the row where it has fewer cells.
fn cell_start(text: &str, line: Line, cell: usize) -> usize {
    if cell == 0 {
        return line.start;
    }
    let mut tabs = 0;
    for (offset, c) in text.chars().skip(line.start).take(line.len).enumerate() {
        if c == '\t' {
            tabs += 1;
            if tabs == cell {
                return line.start + offset + 1;
            }
        }
    }
    line.start + line.len
}

/// Where the cell `cell` of the row `line` ends (before its tab).
fn cell_end(text: &str, line: Line, cell: usize) -> usize {
    let start = cell_start(text, line, cell);
    let rest = text.chars().skip(start).take(line.start + line.len - start).take_while(|c| *c != '\t').count();
    start + rest
}

/// Puts a table of two rows and two columns below the line of `at` (into it where that line is empty and
/// plain); the cursor goes into its first cell. `None` where `at` already stands in a table.
pub fn insert(text: &mut String, look: &mut Formatting, at: usize) -> Option<usize> {
    if in_table(look, text, at) {
        return None;
    }
    let line = lines(text)[line_of(text, at)];
    if line.len == 0 && look.line_kind(line.number) == LineKind::Plain {
        look.replace(text, line.start..line.start, "\t\n\t");
        look.set_lines(line.number..line.number + 2, LineKind::Table);
        return Some(line.start);
    }
    let end = line.start + line.len;
    look.replace(text, end..end, "\n\t\n\t");
    look.set_lines(line.number + 1..line.number + 3, LineKind::Table);
    Some(end + 1)
}

/// Adds an empty row below the row of `at`; the cursor goes into its first cell.
pub fn add_row(text: &mut String, look: &mut Formatting, at: usize) -> Option<usize> {
    let rows = rows_around(look, text, line_of(text, at))?;
    let all = lines(text);
    let line = all[line_of(text, at)];
    let row = format!("\n{}", "\t".repeat(columns(text, &all, &rows) - 1));
    let end = line.start + line.len;
    look.replace(text, end..end, &row);
    look.set_lines(line.number + 1..line.number + 2, LineKind::Table);
    Some(end + 1)
}

/// Takes the row of `at` away (the whole table where it is its only row); the cursor goes to the start of
/// the line that takes its place.
pub fn remove_row(text: &mut String, look: &mut Formatting, at: usize) -> Option<usize> {
    rows_around(look, text, line_of(text, at))?;
    let number = line_of(text, at);
    remove_lines(text, look, number..number + 1)
}

/// Takes the lines `gone` away with their line breaks; the cursor goes to the start of the line that takes
/// their place.
fn remove_lines(text: &mut String, look: &mut Formatting, gone: Range<usize>) -> Option<usize> {
    let all = lines(text);
    let (first, last) = (all[gone.start], all[gone.end - 1]);
    let end = last.start + last.len;
    if gone.end < all.len() {
        // The break after the last line goes, so the next line moves up, keeping its kind.
        let next = look.line_kind(gone.end);
        look.replace(text, first.start..end + 1, "");
        look.set_lines(gone.start..gone.start + 1, next);
        Some(first.start)
    } else if first.start > 0 {
        // The last lines of the text: the break before them goes.
        look.replace(text, first.start - 1..end, "");
        Some(first.start - 1)
    } else {
        look.replace(text, 0..end, "");
        look.set_lines(0..1, LineKind::Plain);
        Some(0)
    }
}

/// Adds an empty column right of the cell of `at` in every row; the cursor goes into the new cell.
pub fn add_column(text: &mut String, look: &mut Formatting, at: usize) -> Option<usize> {
    let rows = rows_around(look, text, line_of(text, at))?;
    let number = line_of(text, at);
    let cell = cell_at(text, lines(text)[number], at);
    // From the last row up, so that the places of the rows above stay as they are.
    for row in rows.rev() {
        let line = lines(text)[row];
        let have = line_text(text, line).split('\t').count();
        let end = line.start + line.len;
        if have <= cell {
            // A short row gets the cells it lacks up to the new one.
            look.replace(text, end..end, &"\t".repeat(cell + 1 - have + 1));
        } else {
            let after = cell_end(text, line, cell);
            look.replace(text, after..after, "\t");
        }
    }
    Some(cell_start(text, lines(text)[number], cell + 1))
}

/// Takes the column of `at` away in every row (the whole table where it is its only column); the cursor
/// goes into the cell that takes its place.
pub fn remove_column(text: &mut String, look: &mut Formatting, at: usize) -> Option<usize> {
    let rows = rows_around(look, text, line_of(text, at))?;
    let number = line_of(text, at);
    let all = lines(text);
    if columns(text, &all, &rows) == 1 {
        return remove_lines(text, look, rows);
    }
    let cell = cell_at(text, all[number], at);
    for row in rows.rev() {
        let line = lines(text)[row];
        let have = line_text(text, line).split('\t').count();
        if cell >= have {
            continue;
        }
        let (start, end) = (cell_start(text, line, cell), cell_end(text, line, cell));
        // The cell goes with the tab before it, the first cell with the tab after it.
        let gone = if cell > 0 { start - 1..end } else if have > 1 { start..end + 1 } else { start..end };
        look.replace(text, gone, "");
    }
    let line = lines(text)[number];
    let left = line_text(text, line).split('\t').count();
    Some(cell_start(text, line, cell.min(left - 1)))
}

/// Where the cursor goes from `at` with Tab: to the start of the next cell, the next row after the last
/// cell of a row; after the last cell of the table a new row is added. With `back` (Shift+Tab) to the
/// start of the cell before. `None` outside a table.
pub fn next_cell(text: &mut String, look: &mut Formatting, at: usize, back: bool) -> Option<usize> {
    let rows = rows_around(look, text, line_of(text, at))?;
    let all = lines(text);
    let line = all[line_of(text, at)];
    let cell = cell_at(text, line, at);
    let have = line_text(text, line).split('\t').count();
    if back {
        return Some(match (cell, line.number > rows.start) {
            (0, true) => {
                let above = all[line.number - 1];
                cell_start(text, above, line_text(text, above).split('\t').count() - 1)
            }
            (0, false) => line.start,
            _ => cell_start(text, line, cell - 1),
        });
    }
    if cell + 1 < have {
        Some(cell_start(text, line, cell + 1))
    } else if line.number + 1 < rows.end {
        Some(all[line.number + 1].start)
    } else {
        add_row(text, look, at)
    }
}

/// Whether Backspace at `at` would join a cell to the one before, or a row to the line above (the letter
/// before is a tab or a line break); in a table that key then does nothing, as in a word processor.
pub fn backspace_joins_cells(text: &str, look: &Formatting, at: usize) -> bool {
    at > 0 && in_table(look, text, at) && matches!(text.chars().nth(at - 1), Some('\t' | '\n'))
}

/// Whether Delete at `at` would join a cell to the one after, or the line below to the row.
pub fn delete_joins_cells(text: &str, look: &Formatting, at: usize) -> bool {
    in_table(look, text, at) && matches!(text.chars().nth(at), Some('\t' | '\n'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str) -> Vec<Vec<String>> {
        text.split('\n').map(|line| line.split('\t').map(str::to_string).collect()).collect()
    }

    fn kinds(look: &Formatting, text: &str) -> Vec<LineKind> {
        (0..text.split('\n').count()).map(|l| look.line_kind(l)).collect()
    }

    #[test]
    fn a_new_table_has_two_rows_and_two_columns_below_the_line() {
        let mut text = "Erze\nnach der Tabelle".to_string();
        let mut look = Formatting::default();
        let cursor = insert(&mut text, &mut look, 2).unwrap();
        assert_eq!(text, "Erze\n\t\n\t\nnach der Tabelle");
        assert_eq!(kinds(&look, &text), [LineKind::Plain, LineKind::Table, LineKind::Table, LineKind::Plain]);
        assert_eq!(cursor, 5, "the cursor in the first cell");
        assert_eq!(insert(&mut text, &mut look, cursor), None, "no table in a table");
        // An empty plain line becomes the table itself.
        let mut text = "Erze\n".to_string();
        let mut look = Formatting::default();
        assert_eq!(insert(&mut text, &mut look, 5), Some(5));
        assert_eq!(text, "Erze\n\t\n\t");
        assert_eq!(kinds(&look, &text), [LineKind::Plain, LineKind::Table, LineKind::Table]);
    }

    #[test]
    fn rows_and_columns_come_and_go_and_the_cells_keep_their_words() {
        let mut text = "Erze\nErz\tMenge\nGold\t12\nfertig".to_string();
        let mut look = Formatting::default();
        look.set_lines(1..3, LineKind::Table);
        // A column right of "Erz".
        let cursor = add_column(&mut text, &mut look, 6).unwrap();
        assert_eq!(rows(&text)[1..3], [vec!["Erz", "", "Menge"], vec!["Gold", "", "12"]]);
        assert_eq!(cursor, 9, "the cursor in the new cell");
        // A row below the first one, with as many cells as the table has.
        let cursor = add_row(&mut text, &mut look, 6).unwrap();
        assert_eq!(rows(&text)[2], ["", "", ""]);
        assert_eq!(kinds(&look, &text)[1..4], [LineKind::Table; 3]);
        assert_eq!(text.chars().nth(cursor - 1), Some('\n'));
        // The empty row goes again.
        remove_row(&mut text, &mut look, cursor).unwrap();
        assert_eq!(text, "Erze\nErz\t\tMenge\nGold\t\t12\nfertig");
        // The middle column goes, then the first one.
        remove_column(&mut text, &mut look, 9).unwrap();
        assert_eq!(text, "Erze\nErz\tMenge\nGold\t12\nfertig");
        let cursor = remove_column(&mut text, &mut look, 6).unwrap();
        assert_eq!(text, "Erze\nMenge\n12\nfertig");
        assert_eq!(cursor, 5);
        assert_eq!(kinds(&look, &text), [LineKind::Plain, LineKind::Table, LineKind::Table, LineKind::Plain]);
        // The last column takes the table with it, the text around stays.
        remove_column(&mut text, &mut look, 5).unwrap();
        assert_eq!(text, "Erze\nfertig");
        assert!(look.is_plain());
    }

    #[test]
    fn a_short_row_gets_the_cells_it_lacks_and_the_last_row_takes_the_table_with_it() {
        let mut text = "Erz\tMenge\tOrt\nGold".to_string();
        let mut look = Formatting::default();
        look.set_lines(0..2, LineKind::Table);
        add_column(&mut text, &mut look, 11).unwrap();
        assert_eq!(rows(&text), [vec!["Erz", "Menge", "Ort", ""], vec!["Gold", "", "", ""]]);
        let mut text = "vorher\nGold\t12".to_string();
        let mut look = Formatting::default();
        look.set_lines(1..2, LineKind::Table);
        assert_eq!(remove_row(&mut text, &mut look, 8), Some(6));
        assert_eq!(text, "vorher");
        assert!(look.is_plain());
    }

    #[test]
    fn tab_goes_from_cell_to_cell_and_adds_a_row_after_the_last() {
        let mut text = "Erz\tMenge\nGold\t12".to_string();
        let mut look = Formatting::default();
        look.set_lines(0..2, LineKind::Table);
        assert_eq!(next_cell(&mut text, &mut look, 1, false), Some(4));
        assert_eq!(next_cell(&mut text, &mut look, 5, false), Some(10), "the next row");
        assert_eq!(next_cell(&mut text, &mut look, 15, true), Some(10));
        assert_eq!(next_cell(&mut text, &mut look, 10, true), Some(4), "back to the last cell of the row above");
        let cursor = next_cell(&mut text, &mut look, 16, false).unwrap();
        assert_eq!(text, "Erz\tMenge\nGold\t12\n\t");
        assert_eq!(cursor, 18);
        assert!(backspace_joins_cells(&text, &look, 4));
        assert!(!backspace_joins_cells(&text, &look, 5));
        assert!(backspace_joins_cells(&text, &look, 10), "a row is not joined to the one above");
        assert!(delete_joins_cells(&text, &look, 9), "nor the next row to it");
        assert!(delete_joins_cells(&text, &look, 3));
        assert!(!delete_joins_cells("Erz\tMenge", &Formatting::default(), 3), "no table, a tab like any letter");
    }

    #[test]
    fn the_letters_of_the_cells_keep_their_look_when_columns_change() {
        let mut text = "Erz\tMenge\nGold\t12".to_string();
        let mut look = Formatting::default();
        look.set_lines(0..2, LineKind::Table);
        look.set_style(4..9, super::super::format::BOLD, true);
        add_column(&mut text, &mut look, 0).unwrap();
        assert_eq!(text, "Erz\t\tMenge\nGold\t\t12");
        let bold: String = text
            .chars()
            .enumerate()
            .filter(|(at, _)| look.style_at(*at) & super::super::format::BOLD != 0)
            .map(|(_, c)| c)
            .collect();
        assert_eq!(bold, "Menge");
    }
}
