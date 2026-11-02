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

//! The purchases of the journal as suggestions for the items of the stock.
//!
//! A suggestion is a purchase that was neither taken over nor thrown away. What the player decided is kept
//! in `lagervorschlaege.json`, one word per purchase; a purchase without an entry is open. Nothing is written
//! into the stock by this module: it only says what could be booked and how.
//!
//! ⚠ A purchase has no number of its own. Its key is the start of the session, the second of the request,
//! the class, the amount and the count of equal purchases before it in that session, so two purchases of one
//! second stay two suggestions and a re-read of a log gives the same keys.
//!
//! ⚠ An item without a name is not suggested. A named item the list of the stock does not know is shown and
//! cannot be taken over. A place the list of the stock does not know is taken over with the name of the log and
//! added to the list (`Proposal::new_place`); a purchase without a place (bought while travelling, or at a number
//! nobody named) is booked without a place, which the player can fill in later.

use std::collections::BTreeMap;

use super::names::{Items, PlaceName, Places};
use super::{Purchase, Session};
use crate::jsonio::{self, Map, Value, map_set};
use crate::paths::Paths;
pub use crate::places::OwnPlace;
use crate::playtime::write_beside;
use crate::stock::{self, Refused};
use crate::stock_data::StockData;

pub const FILE: &str = "lagervorschlaege.json";
pub const FORMAT: i64 = 1;

/// What the player did with a purchase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Adopted,
    Discarded,
}

impl Decision {
    fn word(self) -> &'static str {
        match self {
            Decision::Adopted => "uebernommen",
            Decision::Discarded => "verworfen",
        }
    }

    fn from_word(word: &str) -> Option<Decision> {
        match word {
            "uebernommen" => Some(Decision::Adopted),
            "verworfen" => Some(Decision::Discarded),
            _ => None,
        }
    }
}

/// The decisions by the key of the purchase.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decisions {
    by_key: BTreeMap<String, Decision>,
}

impl Decisions {
    /// From `lagervorschlaege.json`; a missing, damaged or foreign file gives no decisions, an entry with a
    /// word nobody knows is left out.
    pub fn load(paths: &Paths) -> Decisions {
        let valid = |d: &Value| matches!(d, Value::Object(_));
        let Some(Value::Object(map)) = jsonio::load_json(&paths.app_file(FILE), Some(&valid), "suggestions.load")
        else {
            return Decisions::default();
        };
        let Some(Value::Object(entries)) = map.iter().find(|(k, _)| k == "entscheidungen").map(|(_, v)| v) else {
            return Decisions::default();
        };
        let by_key = entries
            .iter()
            .filter_map(|(key, word)| Some((key.clone(), Decision::from_word(word.as_str()?)?)))
            .collect();
        Decisions { by_key }
    }

    /// Writes the file first beside, then renames; reports a failure.
    pub fn save(&self, paths: &Paths) -> bool {
        let entries: Map = self.by_key.iter().map(|(key, d)| (key.clone(), Value::str(d.word()))).collect();
        let mut data: Map = Vec::new();
        map_set(&mut data, "entscheidungen", Value::Object(entries));
        write_beside(paths, FILE, &mut data, FORMAT, "suggestions.save")
    }

    pub fn get(&self, key: &str) -> Option<Decision> {
        self.by_key.get(key).copied()
    }

    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// Marks the purchases with one decision.
    pub fn set(&mut self, keys: &[String], decision: Decision) {
        for key in keys {
            self.by_key.insert(key.clone(), decision);
        }
    }

    /// Opens the purchases again (what "Rückgängig" does).
    pub fn forget(&mut self, keys: &[String]) {
        for key in keys {
            self.by_key.remove(key);
        }
    }
}

/// One purchase offered to the stock.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub key: String,
    /// Seconds since 1970 (UTC) of the request.
    pub time: i64,
    pub class: String,
    pub quantity: u32,
    pub price_total: f64,
    /// A fraction when the total did not divide.
    pub unit_price: f64,
    /// What the list of the stock calls the item, the name to book; `None` when the list does not know it.
    pub stock_name: Option<String>,
    /// The name to show: the one of the stock, else the first one the sources have.
    pub name: String,
    /// The number of the item in the catalog of the stock (`uex`).
    pub uex: Option<i64>,
    pub place: PlaceName,
    /// The star system of a place the list does not know yet, when the program can tell it.
    pub system: Option<String>,
}

impl Proposal {
    /// May it be booked? The item is in the list of the stock and there is something to book.
    pub fn adoptable(&self) -> bool {
        self.stock_name.is_some() && self.quantity >= 1
    }

    /// The place to book it at; empty for a purchase without a place.
    pub fn place_to_book(&self) -> &str {
        match &self.place {
            PlaceName::Listed(name) | PlaceName::Shown(name) => name,
            PlaceName::Unknown(_) => "",
        }
    }

    /// The place that taking it over adds to the list of the stock.
    pub fn new_place(&self) -> Option<OwnPlace> {
        match &self.place {
            PlaceName::Shown(name) => Some(OwnPlace { name: name.clone(), system: self.system.clone() }),
            _ => None,
        }
    }

    /// Is it booked without a place?
    pub fn without_place(&self) -> bool {
        matches!(self.place, PlaceName::Unknown(_))
    }
}

/// The places that taking these suggestions over adds to the list, each once (by name without case), in order.
pub fn new_places<'a>(chosen: impl IntoIterator<Item = &'a Proposal>) -> Vec<OwnPlace> {
    let mut out: Vec<OwnPlace> = Vec::new();
    for place in chosen.into_iter().filter(|p| p.adoptable()).filter_map(Proposal::new_place) {
        if !out.iter().any(|o| o.name.to_lowercase() == place.name.to_lowercase()) {
            out.push(place);
        }
    }
    out
}

/// The open suggestions, the newest first, and how many open purchases had no name and were left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Proposals {
    pub list: Vec<Proposal>,
    pub unnamed: usize,
}

/// The key of a purchase; `nth` counts the equal ones before it in its session.
pub fn key_of(session_start: i64, purchase: &Purchase, nth: usize) -> String {
    format!("{session_start}:{}:{}:{}:{nth}", purchase.time, purchase.class, purchase.quantity)
}

/// The number of the item in the catalog of the stock (`teile` rows carry it).
fn uex_of(item: Option<&Value>) -> Option<i64> {
    match item {
        Some(Value::Int(text)) => text.parse().ok().filter(|n| *n > 0),
        _ => None,
    }
}

/// The open purchases of the sessions as suggestions.
pub fn proposals(
    sessions: &[Session],
    decisions: &Decisions,
    items: &Items,
    places: &Places,
    data: &StockData,
) -> Proposals {
    let mut out = Proposals::default();
    for session in sessions {
        let mut seen: BTreeMap<(i64, &str, u32), usize> = BTreeMap::new();
        for purchase in &session.purchases {
            let nth = seen.entry((purchase.time, purchase.class.as_str(), purchase.quantity)).or_insert(0);
            let key = key_of(session.start, purchase, *nth);
            *nth += 1;
            if decisions.get(&key).is_some() {
                continue;
            }
            let candidates = items.candidates(&purchase.class, &purchase.guid);
            let Some(first) = candidates.first() else {
                out.unnamed += 1;
                continue;
            };
            let choice = candidates.iter().find_map(|name| data.item_choice(name));
            let place = match &purchase.place {
                Some(number) => places.place(number),
                None => PlaceName::Unknown(String::new()),
            };
            let system = match (&place, &purchase.place) {
                (PlaceName::Shown(_), Some(number)) => places.system(number),
                _ => None,
            };
            out.list.push(Proposal {
                key,
                time: purchase.time,
                class: purchase.class.clone(),
                quantity: purchase.quantity,
                price_total: purchase.price_total,
                unit_price: purchase.unit_price(),
                stock_name: choice.map(|c| c.name.clone()),
                name: choice.map_or_else(|| (*first).to_string(), |c| c.name.clone()),
                uex: choice.and_then(|c| uex_of(c.item.as_ref())),
                place,
                system,
            });
        }
    }
    out.list.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.key.cmp(&b.key)));
    out
}

/// Books one suggestion as an item at its place with its price per piece; a purchase that cost nothing
/// is booked without a price. Nothing happens, and an error comes back, when the suggestion cannot be booked.
/// The place is not added to the list here: that is [`new_places`] and `places::add_own`.
pub fn adopt(trade: &mut Vec<Value>, proposal: &Proposal) -> Result<(), Refused> {
    let Some(name) = proposal.stock_name.as_deref() else {
        return Err(Refused::Goods);
    };
    let place = proposal.place_to_book();
    let amount = proposal.quantity.to_string();
    if proposal.unit_price > 0.0 {
        stock::trade_add_item_with_cost(trade, name, &amount, place, false, proposal.uex, proposal.unit_price)
    } else {
        stock::trade_add_item(trade, name, &amount, place, false, proposal.uex, None)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Kind, Movement};
    use super::*;
    use crate::legacy;
    use crate::paths::Host;
    use crate::pypath::Flavor;
    use crate::stock::{cost_of, number_of};

    fn purchase(time: i64, class: &str, quantity: u32, price_total: f64, place: Option<&str>) -> Purchase {
        Purchase {
            time,
            class: class.into(),
            guid: format!("guid-of-{class}"),
            quantity,
            price_total,
            place: place.map(str::to_string),
            shop: "SCShop_Kiosk".into(),
        }
    }

    fn session(start: i64, purchases: Vec<Purchase>) -> Session {
        Session { start, purchases, movements: Vec::new() }
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| ((*a).to_string(), (*b).to_string())).collect()
    }

    /// Two classes the list of the stock knows (`mag`, `mult`), one it does not (`odd`); `blank` has no name.
    fn items() -> Items {
        Items::from_parts(
            pairs(&[
                ("item_Namemag", "Venture Arms"),
                ("item_Namemult", "FBL-8a Arms Arctic Digital"),
                ("item_Nameodd", "Odd Thing"),
            ]),
            Vec::new(),
            Vec::new(),
        )
    }

    fn places() -> Places {
        let learned = [("1", "levski"), ("2", "outpost"), ("3", "Pyro_Outpost_Odd")]
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let list = ["Levski", "Area 18", "Stanton Gateway (Pyro)"].map(String::from);
        Places::from_parts(learned, Default::default(), list)
    }

    /// The stock list of the program: it knows `Venture Arms` (number 171) and `FBL-8a Arms Arctic Digital`
    /// (number 261), not `Odd Thing`.
    fn data() -> StockData {
        StockData::sample()
    }

    fn open(sessions: &[Session], decisions: &Decisions) -> Proposals {
        proposals(sessions, decisions, &items(), &places(), &data())
    }

    #[test]
    fn a_purchase_becomes_a_suggestion_with_name_place_and_price_per_piece() {
        let sessions = [session(100, vec![purchase(150, "mag", 3, 1560.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        assert_eq!(found.list.len(), 1);
        let p = &found.list[0];
        assert_eq!((p.name.as_str(), p.quantity, p.unit_price, p.price_total), ("Venture Arms", 3, 520.0, 1560.0));
        assert_eq!(p.stock_name.as_deref(), Some("Venture Arms"));
        assert_eq!(p.uex, Some(171));
        assert_eq!(p.place, PlaceName::Listed("Levski".into()));
        assert!(p.adoptable());
    }

    #[test]
    fn an_item_without_a_name_is_not_suggested_but_counted() {
        let sessions =
            [session(100, vec![purchase(150, "blank", 1, 10.0, Some("1")), purchase(160, "mag", 1, 10.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        assert_eq!(found.list.len(), 1);
        assert_eq!(found.list[0].class, "mag");
        assert_eq!(found.unnamed, 1);
    }

    #[test]
    fn a_place_outside_the_list_is_taken_over_with_the_name_of_the_log_and_becomes_a_new_place() {
        let sessions =
            [session(100, vec![purchase(150, "mag", 1, 10.0, Some("2")), purchase(160, "mult", 2, 10.0, Some("3"))])];
        let found = open(&sessions, &Decisions::default());
        let (odd, outpost) = (&found.list[0], &found.list[1]);
        assert_eq!(outpost.place, PlaceName::Shown("outpost".into()));
        assert!(outpost.adoptable());
        assert_eq!(outpost.new_place(), Some(OwnPlace { name: "outpost".into(), system: None }));
        // The system comes from the key when the list names it in brackets.
        assert_eq!(odd.new_place(), Some(OwnPlace { name: "Pyro Outpost Odd".into(), system: Some("Pyro".into()) }));
        let mut trade = Vec::new();
        assert_eq!(adopt(&mut trade, outpost), Ok(()));
        assert_eq!(trade[0].get("ort"), Some(&Value::str("outpost")));
        assert_eq!(new_places(&found.list).len(), 2);
    }

    #[test]
    fn a_purchase_without_a_place_is_booked_without_one() {
        let sessions =
            [session(100, vec![purchase(160, "mag", 1, 10.0, Some("99")), purchase(170, "mag", 1, 10.0, None)])];
        let found = open(&sessions, &Decisions::default());
        assert_eq!(found.list[1].place, PlaceName::Unknown("99".into()));
        assert_eq!(found.list[0].place, PlaceName::Unknown(String::new()));
        for p in &found.list {
            assert!(p.adoptable() && p.without_place() && p.new_place().is_none());
            assert_eq!(p.place_to_book(), "");
        }
        let mut trade = Vec::new();
        assert_eq!(adopt(&mut trade, &found.list[0]), Ok(()));
        assert_eq!(adopt(&mut trade, &found.list[1]), Ok(()));
        assert_eq!(trade.len(), 1, "two pieces without a place are one stack");
        assert_eq!(trade[0].get("ort"), Some(&Value::str("")));
        assert_eq!(number_of(&trade[0], "menge"), 2.0);
        assert!(new_places(&found.list).is_empty());
    }

    #[test]
    fn a_new_place_is_named_once_and_only_for_what_can_be_taken_over() {
        let sessions = [session(
            100,
            vec![
                purchase(150, "mag", 1, 10.0, Some("2")),
                purchase(160, "mult", 1, 10.0, Some("2")),
                purchase(170, "odd", 1, 10.0, Some("3")),
            ],
        )];
        let found = open(&sessions, &Decisions::default());
        assert_eq!(new_places(&found.list), vec![OwnPlace { name: "outpost".into(), system: None }]);
    }

    #[test]
    fn a_named_item_the_stock_does_not_list_is_shown_and_cannot_be_taken_over() {
        let sessions = [session(100, vec![purchase(150, "odd", 1, 10.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        assert_eq!(found.list[0].name, "Odd Thing");
        assert_eq!(found.list[0].stock_name, None);
        assert!(!found.list[0].adoptable());
    }

    #[test]
    fn the_stock_names_the_item_the_way_its_list_does_when_a_later_source_matches() {
        // The original name is no entry of the list; the name of the shop is.
        let items = Items::from_parts(
            pairs(&[("item_Namemag", "Venture Arms Original")]),
            pairs(&[("guid-of-mag", "Venture Arms")]),
            Vec::new(),
        );
        let sessions = [session(100, vec![purchase(150, "mag", 1, 10.0, Some("1"))])];
        let found = proposals(&sessions, &Decisions::default(), &items, &places(), &data());
        assert_eq!(found.list[0].stock_name.as_deref(), Some("Venture Arms"));
        assert_eq!(found.list[0].name, "Venture Arms");
    }

    #[test]
    fn what_was_decided_is_not_suggested_again_and_forgetting_opens_it() {
        let sessions =
            [session(100, vec![purchase(150, "mag", 1, 10.0, Some("1")), purchase(160, "mult", 1, 10.0, Some("1"))])];
        let all = open(&sessions, &Decisions::default());
        let keys: Vec<String> = all.list.iter().map(|p| p.key.clone()).collect();
        let mut decisions = Decisions::default();
        decisions.set(&keys[..1], Decision::Adopted);
        decisions.set(&keys[1..], Decision::Discarded);
        assert!(open(&sessions, &decisions).list.is_empty());
        decisions.forget(&keys[..1]);
        let again = open(&sessions, &decisions);
        assert_eq!(again.list.len(), 1);
        assert_eq!(again.list[0].key, keys[0]);
    }

    #[test]
    fn purchases_of_one_second_stay_apart_and_a_second_look_gives_the_same_keys() {
        let twice = vec![purchase(150, "mag", 1, 10.0, Some("1")), purchase(150, "mag", 1, 10.0, Some("1"))];
        let sessions = [session(100, twice.clone())];
        let first = open(&sessions, &Decisions::default());
        assert_eq!(first.list.len(), 2);
        assert_ne!(first.list[0].key, first.list[1].key);
        let again = open(&[session(100, twice)], &Decisions::default());
        let keys = |p: &Proposals| p.list.iter().map(|x| x.key.clone()).collect::<Vec<_>>();
        assert_eq!(keys(&first), keys(&again));
        // Deciding one of the two leaves the other.
        let mut decisions = Decisions::default();
        decisions.set(&keys(&first)[..1], Decision::Discarded);
        assert_eq!(open(&sessions, &decisions).list.len(), 1);
    }

    #[test]
    fn two_purchases_of_one_day_are_two_suggestions() {
        let sessions =
            [session(100, vec![purchase(150, "mag", 1, 10.0, Some("1")), purchase(9000, "mag", 1, 10.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        assert_eq!(found.list.len(), 2);
        assert_eq!(found.list[0].time, 9000, "the newest first");
        // Deciding on one of them leaves the other: the moment is part of the key.
        let mut decisions = Decisions::default();
        decisions.set(&[found.list[0].key.clone()], Decision::Discarded);
        let left = open(&sessions, &decisions);
        assert_eq!(left.list.len(), 1);
        assert_eq!(left.list[0].time, 150);
    }

    #[test]
    fn the_same_purchase_in_two_sessions_is_two_suggestions() {
        let one = purchase(150, "mag", 1, 10.0, Some("1"));
        let sessions = [session(100, vec![one.clone()]), session(500, vec![one])];
        assert_eq!(open(&sessions, &Decisions::default()).list.len(), 2);
    }

    #[test]
    fn adopting_books_the_item_with_its_price_and_place() {
        let sessions = [session(100, vec![purchase(150, "mag", 3, 1560.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        let mut trade = Vec::new();
        assert_eq!(adopt(&mut trade, &found.list[0]), Ok(()));
        assert_eq!(trade.len(), 1);
        assert_eq!(trade[0].get("ware"), Some(&Value::str("Venture Arms")));
        assert_eq!(trade[0].get("ort"), Some(&Value::str("Levski")));
        assert_eq!(trade[0].get("uex"), Some(&Value::int(171)));
        assert_eq!(number_of(&trade[0], "menge"), 3.0);
        assert_eq!(cost_of(&trade[0]), Some(520.0));
        assert!(stock::is_pieces(&trade[0]));
    }

    #[test]
    fn two_purchases_at_one_place_mix_their_price() {
        let sessions =
            [session(100, vec![purchase(150, "mag", 1, 100.0, Some("1")), purchase(160, "mag", 3, 900.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        let mut trade = Vec::new();
        for p in found.list.iter().rev() {
            assert_eq!(adopt(&mut trade, p), Ok(()));
        }
        assert_eq!(trade.len(), 1);
        assert_eq!(number_of(&trade[0], "menge"), 4.0);
        assert_eq!(cost_of(&trade[0]), Some(250.0));
    }

    #[test]
    fn a_purchase_that_cost_nothing_is_booked_without_a_price() {
        let sessions = [session(100, vec![purchase(150, "mag", 2, 0.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        let mut trade = Vec::new();
        assert_eq!(adopt(&mut trade, &found.list[0]), Ok(()));
        assert_eq!(number_of(&trade[0], "menge"), 2.0);
        assert_eq!(cost_of(&trade[0]), None);
    }

    #[test]
    fn a_purchase_of_no_piece_cannot_be_booked() {
        let sessions = [session(100, vec![purchase(150, "mag", 0, 0.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        assert!(!found.list[0].adoptable());
    }

    fn scratch(name: &str) -> (String, Paths) {
        let home = std::env::temp_dir()
            .join(format!("versekit-suggest-{}-{name}", crate::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("folder");
        let host = Host::new(Flavor::native(), &[(legacy::ENV_HOME, &home)], &home);
        (home, Paths::new(host))
    }

    #[test]
    fn the_decisions_are_written_and_read_again() {
        let (home, paths) = scratch("decisions");
        assert!(Decisions::load(&paths).is_empty());
        let mut decisions = Decisions::default();
        decisions.set(&["a:1:mag:1:0".to_string()], Decision::Adopted);
        decisions.set(&["b:2:mult:2:0".to_string()], Decision::Discarded);
        assert!(decisions.save(&paths));
        let read = Decisions::load(&paths);
        assert_eq!(read, decisions);
        assert_eq!(read.get("a:1:mag:1:0"), Some(Decision::Adopted));
        assert_eq!(read.get("b:2:mult:2:0"), Some(Decision::Discarded));
        // A damaged file and an unknown word give nothing (no crash).
        std::fs::write(paths.app_file(FILE), "{kaputt").unwrap();
        assert!(Decisions::load(&paths).is_empty());
        std::fs::write(
            paths.app_file(FILE),
            r#"{"format": 1, "entscheidungen": {"x": "vielleicht", "y": "verworfen"}}"#,
        )
        .unwrap();
        let odd = Decisions::load(&paths);
        assert_eq!((odd.len(), odd.get("y")), (1, Some(Decision::Discarded)));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn the_file_holds_no_name_of_a_player() {
        let (home, paths) = scratch("privacy");
        let mut decisions = Decisions::default();
        decisions.set(&["100:150:mag:1:0".to_string()], Decision::Adopted);
        assert!(decisions.save(&paths));
        let text = std::fs::read_to_string(paths.app_file(FILE)).unwrap();
        assert!(text.contains("100:150:mag:1:0"));
        for forbidden in ["playerId", "shopId", "kioskId", "Handle", "Spieler"] {
            assert!(!text.contains(forbidden), "{forbidden}");
        }
        let _ = std::fs::remove_dir_all(home);
    }

    /// A move is no purchase: it never becomes a suggestion.
    #[test]
    fn moves_of_the_journal_make_no_suggestion() {
        let moved = Movement {
            time: 150,
            kind: Kind::Store,
            class: "mag".into(),
            from: super::super::Endpoint::Nowhere,
            to: super::super::Endpoint::Location("1".into()),
            amount: 1,
            ok: Some(true),
        };
        let sessions = [Session { start: 100, purchases: Vec::new(), movements: vec![moved] }];
        assert!(open(&sessions, &Decisions::default()).list.is_empty());
    }
}
