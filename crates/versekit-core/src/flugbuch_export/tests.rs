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

use super::*;
use crate::flugbuch::{Data, Event};

const SHARD: &str = "pub_euw1b_12881860_420";
/// A moment of 2026-10-01 around 10:00 UTC.
const BASE: i64 = 1_790_848_800;

fn event(at: i64, kind: Kind, data: Data) -> Event {
    Event { at: BASE + at, kind, data }
}

/// A session that carries everything the file keeps, the secrets of the file included.
fn session() -> Session {
    let data = |f: &dyn Fn(&mut Data)| {
        let mut data = Data::default();
        f(&mut data);
        data
    };
    Session {
        start: BASE,
        end: BASE + 1800,
        account: "Xharig".to_string(),
        events: vec![
            event(0, Kind::Start, Data::default()),
            event(5, Kind::Shard, data(&|d| d.shard = Some(SHARD.to_string()))),
            event(10, Kind::Place, data(&|d| d.place = Some("5629".to_string()))),
            event(20, Kind::Target, data(&|d| d.target = Some("rs_ext_nyx-castra_jp1".to_string()))),
            event(30, Kind::Jump, data(&|d| {
                d.ship = Some("RSI_Polaris".to_string());
                d.target = Some("rs_ext_nyx-castra_jp1".to_string());
            })),
            event(40, Kind::Injury, data(&|d| d.severity = Some("schwer".to_string()))),
            event(50, Kind::Death, Data::default()),
            event(60, Kind::Place, data(&|d| d.place = Some("0".to_string()))),
            event(70, Kind::Loading, data(&|d| d.seconds = Some(25))),
            event(80, Kind::Ship, data(&|d| d.ship = Some("ANVL_Asgard".to_string()))),
        ],
    }
}

fn all_topics() -> HashSet<Topic> {
    Topic::ALL.into_iter().collect()
}

fn book_of(session: &Session) -> Book {
    Book::of_sessions(vec![session.clone()])
}

#[test]
fn the_export_has_only_the_whitelist() {
    // The type holds nothing else: a new field makes this stop compiling and asks for a decision.
    let Line { at: _, what: _, place: _, ship: _, title: _, result: _ } = Line::of(0, What::Start);
    let session = session();
    let book = book_of(&session);
    let found = lines(&book, &session, Lang::En, &all_topics());
    let text = render_text(&found, &session, Lang::En);
    let json = render_json(&found, &session);
    for out in [&text, &json] {
        for secret in [SHARD, "12881860", "5629", "420", "Xharig", "35.241"] {
            assert!(!out.contains(secret), "{secret} stands in the export:\n{out}");
        }
    }
    let Ok(Value::Object(root)) = jsonio::loads(&json) else { panic!("an object") };
    let Some(Value::Array(events)) = root.iter().find(|(k, _)| k == "events").map(|(_, v)| v) else { panic!("events") };
    assert!(!events.is_empty());
    for event in events {
        let Value::Object(map) = event else { panic!("an object") };
        for (key, _) in map {
            assert!(["at", "type", "place", "ship", "title", "result"].contains(&key.as_str()), "{key}");
        }
    }
}

#[test]
fn the_export_follows_the_topics_chosen_and_never_carries_a_ship_or_a_loading_screen() {
    let session = session();
    let book = book_of(&session);
    let only_deaths: HashSet<Topic> = [Topic::Death].into_iter().collect();
    let found = lines(&book, &session, Lang::En, &only_deaths);
    assert_eq!(found.iter().map(|l| l.what).collect::<Vec<_>>(), [What::Death]);
    let everything = lines(&book, &session, Lang::En, &all_topics());
    assert_eq!(everything.len(), 8, "the ship and the loading screen are not among them");
}

#[test]
fn a_place_without_a_name_is_no_number_and_the_shard_is_only_said_to_have_changed() {
    let session = session();
    let book = book_of(&session);
    let found = lines(&book, &session, Lang::De, &all_topics());
    let shard = found.iter().find(|l| l.what == What::ShardChanged).expect("a shard line");
    assert_eq!((shard.place.as_deref(), shard.title.as_deref()), (None, None));
    let arrived = found.iter().find(|l| l.what == What::Arrived).expect("an arrival");
    assert_eq!(arrived.place, None, "a number nobody learned is no name");
    let text = render_text(&found, &session, Lang::De);
    assert!(text.contains(&t_in(Lang::De, "s_uv_ort_unbekannt")), "{text}");
    assert!(text.contains("Shard"), "{text}");
}

#[test]
fn the_time_is_utc_and_the_json_names_each_line() {
    let session = session();
    let book = book_of(&session);
    let found = lines(&book, &session, Lang::En, &all_topics());
    let json = render_json(&found, &session);
    assert!(json.contains("\"at\": \"2026-10-01T10:00:00Z\""), "{json}");
    assert!(json.contains("\"type\": \"jump\""), "{json}");
    assert!(json.contains("\"result\": \"severe\""), "{json}");
    let first = render_text(&found, &session, Lang::En);
    assert!(first.contains("2026-10-01T10:00:30Z"), "{first}");
}
