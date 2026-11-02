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

//! The order of every list of parts, weapons and blueprints that carry a size: by size, smallest first, one
//! without a size last, then by name in lower case. Names with a Roman number (Omnisky III, VI, IX) so follow
//! their size, not their letters. A list without sizes keeps the order of its names.

use std::cmp::Ordering;

/// The sort key of one row: no size last, then the size, then the name in lower case.
pub fn key(size: Option<i64>, name: &str) -> (bool, i64, String) {
    (size.is_none(), size.unwrap_or(0), name.to_lowercase())
}

/// Two rows compared as [`key`] orders them.
pub fn compare(a: (Option<i64>, &str), b: (Option<i64>, &str)) -> Ordering {
    key(a.0, a.1).cmp(&key(b.0, b.1))
}

/// The size in a text field (`"3"`, `"3.0"`, `" S3 "`), or `None` if it holds none.
pub fn parse(text: &str) -> Option<i64> {
    let text = crate::text::py_strip(text);
    let digits = text.strip_prefix(['S', 's']).unwrap_or(text);
    let number: f64 = digits.parse().ok()?;
    (number.is_finite() && number >= 0.0).then(|| number.trunc() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_go_by_size_then_name_and_one_without_a_size_last() {
        let mut rows = [
            (Some(3), "Omnisky IX Cannon"),
            (None, "Aaron Cannon"),
            (Some(1), "Omnisky III Cannon"),
            (Some(5), "Omnisky XV Cannon"),
            (Some(2), "Omnisky VI Cannon"),
            (Some(2), "attrition-2 repeater"),
        ];
        rows.sort_by(|a, b| compare(*a, *b));
        let names: Vec<&str> = rows.iter().map(|r| r.1).collect();
        assert_eq!(
            names,
            [
                "Omnisky III Cannon",
                "attrition-2 repeater",
                "Omnisky VI Cannon",
                "Omnisky IX Cannon",
                "Omnisky XV Cannon",
                "Aaron Cannon"
            ],
            "by size, the same size by name without regard to case, no size last"
        );
    }

    #[test]
    fn without_any_size_the_names_decide() {
        let mut rows = [(None, "Pembroke"), (None, "arden"), (None, "Morozov")];
        rows.sort_by(|a, b| compare(*a, *b));
        assert_eq!(rows.iter().map(|r| r.1).collect::<Vec<_>>(), ["arden", "Morozov", "Pembroke"]);
    }

    #[test]
    fn a_size_is_read_from_a_text_field() {
        for (text, wanted) in [("3", Some(3)), (" 3.0 ", Some(3)), ("S12", Some(12)), ("s1", Some(1)), ("0", Some(0))] {
            assert_eq!(parse(text), wanted, "{text:?}");
        }
        for text in ["", " ", "groß", "-1", "NaN", "S"] {
            assert_eq!(parse(text), None, "{text:?}");
        }
    }
}
