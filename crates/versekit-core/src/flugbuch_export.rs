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

//! The flight log of one session as a text or a JSON file the player hands on by hand (a partner, a group). Nothing
//! here sends anything.
//!
//! ⚠ A [`Line`] has exactly the fields the export may carry: the moment (UTC), what happened, a readable name of a
//! place, the class of a ship, the title of a contract or a blueprint and a result. The shard, the address of a
//! server, the number of a place, the handle of anyone and every id are not in the type: they cannot get into the
//! export by a mistake of a filter.

use std::collections::HashSet;

use crate::export::utc_text;
use crate::flugbuch::{Book, Item, Kind, Row, Session, Topic};
use crate::jsonio::{self, Value};
use crate::language::{Lang, t_in, tf_in};
use crate::mission_log;

pub const FORMAT: i64 = 1;

/// What a line tells; the names are the ones of the JSON file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    Start,
    End,
    Crash,
    ShardChanged,
    ServerChange,
    Arrived,
    Underway,
    Target,
    Jump,
    Death,
    Injury,
    ConnectionLost,
    ContractAccepted,
    ContractEnded,
    BlueprintFound,
}

impl What {
    pub fn stored(self) -> &'static str {
        match self {
            What::Start => "session_start",
            What::End => "session_end",
            What::Crash => "crash",
            What::ShardChanged => "shard_changed",
            What::ServerChange => "server_change",
            What::Arrived => "arrived",
            What::Underway => "underway",
            What::Target => "target",
            What::Jump => "jump",
            What::Death => "death",
            What::Injury => "injury",
            What::ConnectionLost => "connection_lost",
            What::ContractAccepted => "contract_accepted",
            What::ContractEnded => "contract_ended",
            What::BlueprintFound => "blueprint_found",
        }
    }

    /// The key of the sentence in the text export.
    fn text_key(self) -> &'static str {
        match self {
            What::Start => "s_flb_e_start",
            What::End => "s_flb_e_ende",
            What::Crash => "s_flb_e_absturz",
            What::ShardChanged => "s_flb_e_shard",
            What::ServerChange => "s_flb_e_server",
            What::Arrived => "s_flb_e_ort",
            What::Underway => "s_uv_unterwegs",
            What::Target => "s_flb_e_ziel",
            What::Jump => "s_flb_e_sprung",
            What::Death => "s_flb_e_tod",
            What::Injury => "s_flb_e_verletzung",
            What::ConnectionLost => "s_flb_e_verbindung",
            What::ContractAccepted => "s_flb_e_auftrag",
            What::ContractEnded => "s_flb_e_auftrag_ende",
            What::BlueprintFound => "s_flb_e_fund",
        }
    }
}

/// One line of the export: the moment and what happened, and of the whitelist what belongs to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Seconds since 1970, UTC.
    pub at: i64,
    pub what: What,
    /// The readable name of a place: arrived at, or the target of a jump.
    pub place: Option<String>,
    /// The readable name of the class of a ship.
    pub ship: Option<String>,
    /// The title of a contract, or the name of a blueprint.
    pub title: Option<String>,
    /// How a contract ended (`completed`, `aborted`, `failed`, `expired`) or how bad an injury is (`minor`,
    /// `moderate`, `severe`).
    pub result: Option<String>,
}

impl Line {
    fn of(at: i64, what: What) -> Line {
        Line { at, what, place: None, ship: None, title: None, result: None }
    }
}

fn contract_result(state: &str) -> &'static str {
    match state {
        mission_log::COMPLETED => "completed",
        mission_log::ABORTED => "aborted",
        mission_log::FAILED => "failed",
        mission_log::EXPIRED => "expired",
        _ => "running",
    }
}

fn injury_result(stored: &str) -> &'static str {
    match stored {
        "leicht" => "minor",
        "mittel" => "moderate",
        _ => "severe",
    }
}

/// The lines of a session for the topics chosen, in time order. A topic that cannot be exported is left out
/// whether it was chosen or not.
pub fn lines(book: &Book, session: &Session, lang: Lang, topics: &HashSet<Topic>) -> Vec<Line> {
    book.rows(session)
        .iter()
        .filter(|row| row.topic().exportable() && topics.contains(&row.topic()))
        .filter_map(|row| line_of(book, row, lang))
        .collect()
}

/// The line of one row of the timeline; `None` for a row the export does not carry (a ship, a loading screen).
pub fn line_of(book: &Book, row: &Row, lang: Lang) -> Option<Line> {
    let at = row.at;
    Some(match &row.item {
        Item::Accepted { name } => Line { title: Some(name.clone()), ..Line::of(at, What::ContractAccepted) },
        Item::Ended { name, state } => Line {
            title: Some(name.clone()),
            result: Some(contract_result(state).to_string()),
            ..Line::of(at, What::ContractEnded)
        },
        Item::Find { name, .. } => Line { title: Some(name.clone()), ..Line::of(at, What::BlueprintFound) },
        Item::Event(event) => match event.kind {
            Kind::Start => Line::of(at, What::Start),
            Kind::End => Line::of(at, What::End),
            Kind::Crash => Line::of(at, What::Crash),
            Kind::Shard => Line::of(at, What::ShardChanged),
            Kind::ServerChange => Line::of(at, What::ServerChange),
            Kind::Place => match event.data.place.as_deref() {
                Some(number) if number != crate::location::NOWHERE => {
                    Line { place: book.place_name(number), ..Line::of(at, What::Arrived) }
                }
                _ => Line::of(at, What::Underway),
            },
            Kind::Target => {
                Line { place: book.target_name(lang, event.data.target.as_deref()), ..Line::of(at, What::Target) }
            }
            Kind::Jump => Line {
                place: book.target_name(lang, event.data.target.as_deref()),
                ship: event.data.ship.as_deref().map(|class| book.ship_name(class)),
                ..Line::of(at, What::Jump)
            },
            Kind::Death => Line::of(at, What::Death),
            Kind::Injury => Line {
                result: event.data.severity.as_deref().map(|s| injury_result(s).to_string()),
                ..Line::of(at, What::Injury)
            },
            Kind::Connection => Line::of(at, What::ConnectionLost),
            Kind::Loading | Kind::Ship => return None,
        },
    })
}

/// The sentence of a line in `lang`.
pub fn sentence(line: &Line, lang: Lang) -> String {
    let place = line.place.clone().unwrap_or_else(|| t_in(lang, "s_uv_ort_unbekannt"));
    let mut text = match line.what {
        What::Arrived | What::Target | What::Jump => {
            tf_in(lang, line.what.text_key(), &[("0", place.as_str().into())])
        }
        What::ContractAccepted | What::ContractEnded | What::BlueprintFound => tf_in(
            lang,
            line.what.text_key(),
            &[("0", line.title.as_deref().unwrap_or_default().into())],
        ),
        _ => t_in(lang, line.what.text_key()),
    };
    if line.what == What::Jump && line.place.is_none() {
        text = t_in(lang, "s_flb_e_sprung_ohne");
    }
    if let Some(ship) = &line.ship {
        text = format!("{text} · {ship}");
    }
    if let Some(result) = &line.result {
        text = format!("{text} · {}", t_in(lang, &format!("s_flb_r_{result}")));
    }
    text
}

/// The text export: a head line with the day and one line per event, `moment (UTC)  sentence`.
pub fn render_text(lines: &[Line], session: &Session, lang: Lang) -> String {
    let mut out = tf_in(lang, "s_flb_x_kopf", &[("0", utc_text(session.start, false).as_str().into())]);
    out.push('\n');
    for line in lines {
        out.push('\n');
        out.push_str(&format!("{}  {}", utc_text(line.at, false), sentence(line, lang)));
    }
    out.push('\n');
    out
}

/// The JSON export: `{"format", "session": {"start", "end"}, "events": [{"at", "type", …}]}`; a field a line does
/// not have is left out.
pub fn render_json(lines: &[Line], session: &Session) -> String {
    let events = lines
        .iter()
        .map(|line| {
            let mut map = vec![("at".to_string(), Value::str(utc_text(line.at, false))), ("type".into(), Value::str(line.what.stored()))];
            for (key, value) in [("place", &line.place), ("ship", &line.ship), ("title", &line.title), ("result", &line.result)] {
                if let Some(text) = value {
                    map.push((key.into(), Value::str(text.as_str())));
                }
            }
            Value::Object(map)
        })
        .collect();
    let root = Value::Object(vec![
        ("format".into(), Value::int(FORMAT)),
        (
            "session".into(),
            Value::Object(vec![
                ("start".into(), Value::str(utc_text(session.start, false))),
                ("end".into(), Value::str(utc_text(session.end, false))),
            ]),
        ),
        ("events".into(), Value::Array(events)),
    ]);
    jsonio::dumps(&root, Some(2), false)
}

#[cfg(test)]
mod tests;
