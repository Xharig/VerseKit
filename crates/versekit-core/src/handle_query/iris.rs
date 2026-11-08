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

//! The lists of another tool for marking players and organizations (the TOML form `sc-social-overlay`, version 1),
//! read so that such a list can be taken over: its players by handle and its organizations by SID, each with a
//! colour. The tool knows no stances, only colours; [`stance_of_colour`] gives each colour the nearest stance.
//!
//! Only the few forms of such a list are read, line by line: `key = "text"`, `key = number` and the tables
//! `[[players]]`, `[[orgs]]`; the rules of a list (`[[org_rules]]`) and every other table are passed over.

use crate::handle_query::relations::{Kind, Stance};

/// The mark of the form in the head of such a list.
pub const FORMAT: &str = "sc-social-overlay";
/// The version of the form that is read.
pub const VERSION: i64 = 1;

/// The stance a colour of such a list stands for: red and orange hostile, green, cyan and blue friendly, yellow,
/// magenta and purple to watch, grey neutral. An unknown colour gives none.
pub fn stance_of_colour(colour: &str) -> Option<Stance> {
    match colour.trim().to_lowercase().as_str() {
        "red" | "orange" => Some(Stance::Hostile),
        "green" | "cyan" | "blue" => Some(Stance::Friendly),
        "yellow" | "magenta" | "purple" => Some(Stance::Watch),
        "gray" | "grey" => Some(Stance::Neutral),
        _ => None,
    }
}

/// The table a line stands in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Table {
    Head,
    Player,
    Org,
    Other,
}

/// The value of `key = value`: a text in double or single quotes without its quotes, else the first word; a comment
/// after it goes.
fn value_of(raw: &str) -> String {
    let raw = raw.trim();
    for quote in ['"', '\''] {
        if let Some(rest) = raw.strip_prefix(quote) {
            return rest.split(quote).next().unwrap_or("").to_string();
        }
    }
    raw.split(['#', ' ', '\t']).next().unwrap_or("").to_string()
}

/// A key without the quotes it may stand in.
fn key_of(raw: &str) -> &str {
    raw.trim().trim_matches(['"', '\''])
}

/// The name of an array of tables, `[[ players ]]` → `players`; `None` for another line.
fn table_of(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("[[")?;
    let end = rest.find("]]")?;
    Some(key_of(&rest[..end]))
}

/// One entry of the list while it is read.
#[derive(Default)]
struct Entry {
    name: Option<String>,
    colour: Option<String>,
}

/// The marks of a list in the form [`FORMAT`]: kind, name and stance, in the order of the list. `None` when the text
/// is no such list (no head, another form or version); entries without a name or with a colour that is no stance
/// are left out.
pub fn read(text: &str) -> Option<Vec<(Kind, String, Stance)>> {
    let mut table = Table::Head;
    let (mut format, mut version) = (None, None);
    let mut entries: Vec<(Kind, Entry)> = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            table = match table_of(line) {
                Some("players") => Table::Player,
                Some("orgs") => Table::Org,
                _ => Table::Other,
            };
            match table {
                Table::Player => entries.push((Kind::Handle, Entry::default())),
                Table::Org => entries.push((Kind::Org, Entry::default())),
                _ => {}
            }
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else { continue };
        let (key, value) = (key_of(key), value_of(raw));
        match table {
            Table::Head if key == "format" => format = Some(value),
            Table::Head if key == "format_version" => version = value.parse::<i64>().ok(),
            Table::Player | Table::Org => {
                let Some((kind, entry)) = entries.last_mut() else { continue };
                match (key, *kind) {
                    ("handle", Kind::Handle) | ("sid", Kind::Org) => entry.name = Some(value),
                    ("color" | "colour", _) => entry.colour = Some(value),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if format.as_deref() != Some(FORMAT) || version != Some(VERSION) {
        return None;
    }
    Some(
        entries
            .into_iter()
            .filter_map(|(kind, entry)| {
                let stance = stance_of_colour(entry.colour.as_deref()?)?;
                Some((kind, entry.name?, stance))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "\u{feff}# A list made up for the checks\n\
        format = \"sc-social-overlay\"\n\
        format_version = 1\n\
        name = \"Beispiel\"   # its name\n\
        \n\
        [[players]]\n\
        handle = \"Beispiel_Pilot\"\n\
        color = \"red\"\n\
        \n\
        [[players]]\n\
        handle = \"Freund-1\"\n\
        color = \"Green\"\n\
        \n\
        [[orgs]]\n\
        sid = \"BEISPIEL\"\n\
        priority = 2\n\
        color = \"yellow\"\n\
        \n\
        [[org_rules]]\n\
        kind = \"no_org\"\n\
        priority = 5\n\
        color = \"gray\"\n\
        \n\
        [[players]]\n\
        handle = \"OhneFarbe\"\n\
        color = \"pink\"\n";

    #[test]
    fn a_list_of_the_other_tool_gives_its_players_and_organizations_with_a_stance() {
        let read = read(LIST).expect("a list");
        assert_eq!(
            read,
            [
                (Kind::Handle, "Beispiel_Pilot".to_string(), Stance::Hostile),
                (Kind::Handle, "Freund-1".to_string(), Stance::Friendly),
                (Kind::Org, "BEISPIEL".to_string(), Stance::Watch),
            ],
            "the rules and an unknown colour are left out"
        );
    }

    /// The other ways TOML allows to write the same: single quotes, keys in quotes, spaces in the head of a table, a
    /// comment behind it, Windows line ends.
    #[test]
    fn the_other_ways_of_writing_the_same_list_are_read_alike() {
        let list = "format = 'sc-social-overlay'\r\n\"format_version\" = 1\r\n\
                    [[ players ]]   # the first one [1]\r\n\
                    'handle' = 'Einfach_1'\r\n\
                    color = 'orange'   # a comment\r\n\
                    [[orgs]]\r\n\"sid\" = \"ZWEI\"\r\n\"color\" = \"cyan\"\r\n";
        assert_eq!(
            read(list).expect("a list"),
            [(Kind::Handle, "Einfach_1".to_string(), Stance::Hostile), (Kind::Org, "ZWEI".to_string(), Stance::Friendly)]
        );
    }

    #[test]
    fn another_form_or_version_is_no_such_list() {
        assert_eq!(read("format = \"something-else\"\nformat_version = 1\n"), None);
        assert_eq!(read("format = \"sc-social-overlay\"\nformat_version = 2\n"), None);
        assert_eq!(read("{\"format\": \"versekit-beziehungen\"}"), None);
        assert_eq!(read(""), None);
    }

    #[test]
    fn every_colour_has_its_stance() {
        for (colour, stance) in [
            ("red", Some(Stance::Hostile)),
            ("Orange", Some(Stance::Hostile)),
            ("green", Some(Stance::Friendly)),
            ("cyan", Some(Stance::Friendly)),
            ("blue", Some(Stance::Friendly)),
            ("yellow", Some(Stance::Watch)),
            ("magenta", Some(Stance::Watch)),
            ("purple", Some(Stance::Watch)),
            ("gray", Some(Stance::Neutral)),
            ("pink", None),
        ] {
            assert_eq!(stance_of_colour(colour), stance, "{colour}");
        }
    }
}
