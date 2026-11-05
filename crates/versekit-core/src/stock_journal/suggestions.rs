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

//! The purchases and sales of the journal as suggestions for the items of the stock.
//!
//! A suggestion is a purchase or a sale that was neither taken over nor thrown away. What the player decided is
//! kept in `lagervorschlaege.json`, one word per deal; a deal without an entry is open. Nothing is written into
//! the stock by this module on its own: it says what could be booked and how, and books what it is asked to.
//!
//! ⚠ A sale takes its pieces out of the stock: first from the stacks of the item at the place of the sale, then
//! from the stacks of the item without a place, the ones not marked stolen first in each. A stack that runs empty
//! is removed. When the stock holds fewer pieces than were sold, what is there is taken; when it holds none, the
//! sale cannot be taken over. Its key starts with `verkauf:`, so a sale and a purchase never share one.
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
use super::{Deal, Purchase, Sale, Session};
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

/// One purchase or sale offered to the stock.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub key: String,
    /// A sale: taking it over takes the pieces out of the stock instead of booking them in.
    pub sold: bool,
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
    /// May it be booked? The item is in the list of the stock and there is something to book. Whether the stock
    /// holds a piece to take out for a sale is told by [`Proposal::bookable`].
    pub fn adoptable(&self) -> bool {
        self.stock_name.is_some() && self.quantity >= 1
    }

    /// May it be booked into or out of this stock? A sale needs at least one piece of its item there.
    pub fn bookable(&self, trade: &[Value]) -> bool {
        self.adoptable() && (!self.sold || take_plan(trade, self).pieces > 0)
    }

    /// The place to book it at (for a sale: the place it was sold at); empty for a deal without a place.
    pub fn place_to_book(&self) -> &str {
        match &self.place {
            PlaceName::Listed(name) | PlaceName::Shown(name) => name,
            PlaceName::Unknown(_) => "",
        }
    }

    /// The place that taking it over adds to the list of the stock; a sale adds none.
    pub fn new_place(&self) -> Option<OwnPlace> {
        match &self.place {
            PlaceName::Shown(name) if !self.sold => Some(OwnPlace { name: name.clone(), system: self.system.clone() }),
            _ => None,
        }
    }

    /// Is a purchase booked without a place?
    pub fn without_place(&self) -> bool {
        !self.sold && matches!(self.place, PlaceName::Unknown(_))
    }
}

/// Where the pieces of a sale come from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Take {
    /// The index of a stack in the trade list and the pieces taken from it, in the order they are taken.
    pub parts: Vec<(usize, u32)>,
    /// All pieces taken; fewer than were sold when the stock holds fewer.
    pub pieces: u32,
    /// Some of them come from a stack without a place.
    pub without_place: bool,
}

fn same_text(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// The stacks a sale takes its pieces from: the item at the place of the sale, then the item without a place,
/// the ones not marked stolen first in each; empty for a purchase or an item the stock list does not know.
pub fn take_plan(trade: &[Value], proposal: &Proposal) -> Take {
    let mut plan = Take::default();
    let Some(name) = proposal.stock_name.as_deref().filter(|_| proposal.sold) else { return plan };
    let place = proposal.place_to_book();
    let text = |entry: &Value, key: &str| entry.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let mut stacks: Vec<(u8, bool, usize)> = trade
        .iter()
        .enumerate()
        .filter(|(_, e)| stock::is_pieces(e) && same_text(&text(e, "ware"), name))
        .filter_map(|(i, e)| {
            let at = text(e, "ort");
            let tier = if !place.trim().is_empty() && same_text(&at, place) {
                0
            } else if at.trim().is_empty() {
                1
            } else {
                return None;
            };
            Some((tier, matches!(e.get("gestohlen"), Some(Value::Bool(true))), i))
        })
        .collect();
    stacks.sort_unstable();
    let mut left = proposal.quantity;
    for (tier, _, index) in stacks {
        if left == 0 {
            break;
        }
        let there = stock::number_of(&trade[index], "menge").max(0.0).floor();
        let taken = if there >= f64::from(left) { left } else { there as u32 };
        if taken == 0 {
            continue;
        }
        plan.parts.push((index, taken));
        plan.pieces += taken;
        plan.without_place |= tier == 1;
        left -= taken;
    }
    plan
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

/// The open suggestions, the newest first, and how many open deals had no name and were left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Proposals {
    pub list: Vec<Proposal>,
    pub unnamed: usize,
}

/// The key of a purchase; `nth` counts the equal ones before it in its session.
pub fn key_of(session_start: i64, purchase: &Purchase, nth: usize) -> String {
    format!("{session_start}:{}:{}:{}:{nth}", purchase.time, purchase.class, purchase.quantity)
}

/// The key of a sale: the one a purchase would have, after `verkauf:`.
pub fn sale_key_of(session_start: i64, sale: &Sale, nth: usize) -> String {
    format!("verkauf:{}", key_of(session_start, sale, nth))
}

/// The number of the item in the catalog of the stock (`teile` rows carry it).
fn uex_of(item: Option<&Value>) -> Option<i64> {
    match item {
        Some(Value::Int(text)) => text.parse().ok().filter(|n| *n > 0),
        _ => None,
    }
}

/// What the sources and the lists of the stock know about one deal.
struct Lookup<'a> {
    decisions: &'a Decisions,
    items: &'a Items,
    places: &'a Places,
    data: &'a StockData,
}

impl Lookup<'_> {
    /// Adds the deal to the open suggestions unless it was decided; counts it when nothing names its item.
    fn offer(&self, out: &mut Proposals, key: String, deal: &Deal, sold: bool) {
        if self.decisions.get(&key).is_some() {
            return;
        }
        let candidates = self.items.candidates(&deal.class, &deal.guid);
        let Some(first) = candidates.first() else {
            out.unnamed += 1;
            return;
        };
        let choice = candidates.iter().find_map(|name| self.data.item_choice(name));
        let place = match &deal.place {
            Some(number) => self.places.place(number),
            None => PlaceName::Unknown(String::new()),
        };
        let system = match (&place, &deal.place) {
            (PlaceName::Shown(_), Some(number)) => self.places.system(number),
            _ => None,
        };
        out.list.push(Proposal {
            key,
            sold,
            time: deal.time,
            class: deal.class.clone(),
            quantity: deal.quantity,
            price_total: deal.price_total,
            unit_price: deal.unit_price(),
            stock_name: choice.map(|c| c.name.clone()),
            name: choice.map_or_else(|| (*first).to_string(), |c| c.name.clone()),
            uex: choice.and_then(|c| uex_of(c.item.as_ref())),
            place,
            system,
        });
    }
}

/// The open purchases and sales of the sessions as suggestions.
pub fn proposals(
    sessions: &[Session],
    decisions: &Decisions,
    items: &Items,
    places: &Places,
    data: &StockData,
) -> Proposals {
    let lookup = Lookup { decisions, items, places, data };
    let mut out = Proposals::default();
    for session in sessions {
        for (deals, sold) in [(&session.purchases, false), (&session.sales, true)] {
            let mut seen: BTreeMap<(i64, &str, u32), usize> = BTreeMap::new();
            for deal in deals {
                let nth = seen.entry((deal.time, deal.class.as_str(), deal.quantity)).or_insert(0);
                let key = if sold { sale_key_of(session.start, deal, *nth) } else { key_of(session.start, deal, *nth) };
                *nth += 1;
                lookup.offer(&mut out, key, deal, sold);
            }
        }
    }
    out.list.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.key.cmp(&b.key)));
    out
}

/// The suggestions that booking them one after the other, oldest first, would book: a sale counts the pieces
/// the purchases before it booked in. The trade list itself stays as it is.
pub fn batch<'a>(trade: &[Value], chosen: &'a [Proposal]) -> Vec<&'a Proposal> {
    let mut order: Vec<&Proposal> = chosen.iter().filter(|p| p.adoptable()).collect();
    order.sort_by(|a, b| a.time.cmp(&b.time).then_with(|| a.key.cmp(&b.key)));
    let mut trial = trade.to_vec();
    order.into_iter().filter(|p| adopt(&mut trial, p).is_ok()).collect()
}

/// Takes the pieces of a sale out of the stock as [`take_plan`] finds them; a stack that runs empty is removed.
fn take_out(trade: &mut Vec<Value>, proposal: &Proposal) -> Result<(), Refused> {
    let plan = take_plan(trade, proposal);
    if plan.pieces == 0 {
        return Err(Refused::Gone);
    }
    let mut empty = Vec::new();
    for (index, taken) in plan.parts {
        let left = stock::number_of(&trade[index], "menge") - f64::from(taken);
        if left < 1.0 {
            empty.push(index);
        } else if let Value::Object(entry) = &mut trade[index] {
            map_set(entry, "menge", Value::Float(left));
        }
    }
    empty.sort_unstable();
    for index in empty.into_iter().rev() {
        trade.remove(index);
    }
    Ok(())
}

/// Books one suggestion: a purchase as an item at its place with its price per piece (one that cost nothing
/// without a price), a sale out of the stock (see [`take_plan`]). Nothing happens, and an error comes back, when
/// the suggestion cannot be booked. The place is not added to the list here: that is [`new_places`] and
/// `places::add_own`.
pub fn adopt(trade: &mut Vec<Value>, proposal: &Proposal) -> Result<(), Refused> {
    let Some(name) = proposal.stock_name.as_deref() else {
        return Err(Refused::Goods);
    };
    if proposal.sold {
        return take_out(trade, proposal);
    }
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
        Session { start, purchases, sales: Vec::new(), movements: Vec::new() }
    }

    fn sold(start: i64, sales: Vec<Sale>) -> Session {
        Session { start, purchases: Vec::new(), sales, movements: Vec::new() }
    }

    /// An item stack of the stock.
    fn stack(name: &str, amount: f64, place: &str, stolen: bool) -> Value {
        Value::Object(vec![
            ("ware".into(), Value::str(name)),
            ("menge".into(), Value::Float(amount)),
            ("ort".into(), Value::str(place)),
            ("gestohlen".into(), Value::Bool(stolen)),
            ("stueck".into(), Value::Bool(true)),
        ])
    }

    fn amounts(trade: &[Value]) -> Vec<(String, f64)> {
        trade
            .iter()
            .map(|e| (e.get("ort").and_then(Value::as_str).unwrap_or("").to_string(), number_of(e, "menge")))
            .collect()
    }

    #[test]
    fn a_sale_becomes_a_suggestion_to_take_out_with_its_price_per_piece() {
        let sessions = [sold(100, vec![purchase(150, "mag", 2, 124.0, Some("1"))])];
        let found = open(&sessions, &Decisions::default());
        assert_eq!(found.list.len(), 1);
        let p = &found.list[0];
        assert!(p.sold);
        assert!(p.key.starts_with("verkauf:"), "{}", p.key);
        assert_eq!((p.name.as_str(), p.quantity, p.unit_price), ("Venture Arms", 2, 62.0));
        assert_eq!(p.place, PlaceName::Listed("Levski".into()));
        assert!(p.adoptable());
        // A place outside the list adds nothing for a sale, and a sale is never booked "without a place".
        let outside = open(&[sold(100, vec![purchase(150, "mag", 1, 9.0, Some("2"))])], &Decisions::default());
        assert_eq!(outside.list[0].new_place(), None);
        assert!(new_places(&outside.list).is_empty());
        let nowhere = open(&[sold(100, vec![purchase(150, "mag", 1, 9.0, None)])], &Decisions::default());
        assert!(!nowhere.list[0].without_place());
    }

    #[test]
    fn a_sale_and_a_purchase_of_the_same_moment_have_their_own_keys() {
        let deal = purchase(150, "mag", 1, 10.0, Some("1"));
        let both = Session { start: 100, purchases: vec![deal.clone()], sales: vec![deal], movements: Vec::new() };
        let found = open(std::slice::from_ref(&both), &Decisions::default());
        assert_eq!(found.list.len(), 2);
        assert_ne!(found.list[0].key, found.list[1].key);
        let sale = found.list.iter().find(|p| p.sold).expect("sale");
        let mut decisions = Decisions::default();
        decisions.set(std::slice::from_ref(&sale.key), Decision::Discarded);
        let left = open(&[both], &decisions);
        assert_eq!(left.list.len(), 1);
        assert!(!left.list[0].sold, "discarding the sale leaves the purchase");
    }

    #[test]
    fn a_sale_takes_from_its_place_first_then_without_a_place_and_stolen_last() {
        let sale = &open(&[sold(100, vec![purchase(150, "mag", 5, 50.0, Some("1"))])], &Decisions::default()).list[0];
        let mut trade = vec![
            stack("Venture Arms", 4.0, "Area 18", false),
            stack("Venture Arms", 2.0, "", false),
            stack("Venture Arms", 1.0, "Levski", true),
            stack("venture arms ", 2.0, "levski", false),
            stack("FBL-8a Arms Arctic Digital", 3.0, "Levski", false),
        ];
        let plan = take_plan(&trade, sale);
        assert_eq!(plan.parts, vec![(3, 2), (2, 1), (1, 2)], "place before no place, not stolen before stolen");
        assert_eq!((plan.pieces, plan.without_place), (5, true));
        assert!(sale.bookable(&trade));
        assert_eq!(adopt(&mut trade, sale), Ok(()));
        // The emptied stacks are gone; the stack at another place and the other item stay as they were.
        assert_eq!(amounts(&trade), vec![("Area 18".to_string(), 4.0), ("Levski".to_string(), 3.0)]);
        assert_eq!(trade[1].get("ware"), Some(&Value::str("FBL-8a Arms Arctic Digital")));
    }

    #[test]
    fn a_sale_takes_what_is_there_and_keeps_the_rest_and_its_price() {
        let sale = &open(&[sold(100, vec![purchase(150, "mag", 2, 50.0, Some("1"))])], &Decisions::default()).list[0];
        let mut big = vec![stack("Venture Arms", 5.0, "Levski", false)];
        if let Value::Object(entry) = &mut big[0] {
            map_set(entry, "einkauf", Value::Float(30.0));
        }
        assert_eq!(adopt(&mut big, sale), Ok(()));
        assert_eq!(number_of(&big[0], "menge"), 3.0);
        assert_eq!(cost_of(&big[0]), Some(30.0), "what was paid for the rest stays");
        // Fewer in stock than sold: what is there is taken.
        let mut small = vec![stack("Venture Arms", 1.0, "Levski", false)];
        assert_eq!(take_plan(&small, sale).pieces, 1);
        assert_eq!(adopt(&mut small, sale), Ok(()));
        assert!(small.is_empty());
    }

    #[test]
    fn all_at_once_books_oldest_first_so_a_sale_takes_what_a_purchase_before_it_brought() {
        let both = Session {
            start: 100,
            purchases: vec![purchase(150, "mag", 3, 30.0, Some("1"))],
            sales: vec![purchase(160, "mag", 2, 40.0, Some("1")), purchase(140, "mult", 1, 5.0, Some("1"))],
            movements: Vec::new(),
        };
        let found = open(&[both], &Decisions::default());
        assert_eq!(found.list.len(), 3);
        let trade: Vec<Value> = Vec::new();
        // The sale of `mult` comes before anything was bought and finds nothing; the sale of `mag` finds the purchase.
        let booked: Vec<(i64, bool)> = batch(&trade, &found.list).iter().map(|p| (p.time, p.sold)).collect();
        assert_eq!(booked, vec![(150, false), (160, true)]);
        assert!(trade.is_empty(), "the trial leaves the list alone");
        let mut real = Vec::new();
        for p in batch(&trade, &found.list) {
            assert_eq!(adopt(&mut real, p), Ok(()));
        }
        assert_eq!(amounts(&real), vec![("Levski".to_string(), 1.0)]);
    }

    #[test]
    fn a_sale_of_an_item_the_stock_does_not_hold_there_cannot_be_taken_over() {
        let sale = &open(&[sold(100, vec![purchase(150, "mag", 1, 50.0, Some("1"))])], &Decisions::default()).list[0];
        let mut elsewhere = vec![stack("Venture Arms", 4.0, "Area 18", false), stack("TS-2", 2.0, "Levski", false)];
        assert!(sale.adoptable() && !sale.bookable(&elsewhere));
        let before = elsewhere.clone();
        assert_eq!(adopt(&mut elsewhere, sale), Err(Refused::Gone));
        assert_eq!(elsewhere, before, "nothing changed");
        // Goods of the same name are no item stack.
        let mut goods = stack("Venture Arms", 4.0, "Levski", false);
        if let Value::Object(entry) = &mut goods {
            entry.retain(|(k, _)| k != "stueck");
        }
        assert!(!sale.bookable(&[goods]));
        // A sale without a place only takes from stacks without a place.
        let flying = &open(&[sold(100, vec![purchase(150, "mag", 1, 9.0, None)])], &Decisions::default()).list[0];
        assert!(!flying.bookable(&[stack("Venture Arms", 4.0, "Levski", false)]));
        assert!(flying.bookable(&[stack("Venture Arms", 4.0, "", false)]));
        // A purchase is bookable without any stock.
        let bought = &open(&[session(100, vec![purchase(150, "mag", 1, 9.0, Some("1"))])], &Decisions::default()).list[0];
        assert!(bought.bookable(&[]));
        assert_eq!(take_plan(&[stack("Venture Arms", 4.0, "Levski", false)], bought), Take::default());
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
        let sessions = [Session { start: 100, purchases: Vec::new(), sales: Vec::new(), movements: vec![moved] }];
        assert!(open(&sessions, &Decisions::default()).list.is_empty());
    }
}
