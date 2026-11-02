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

//! What is still to be got for the ships and the farm notes, and whether it is
//! bought or built: the calculating half of `cart.py` of 4.12 (the bill, the
//! total, the shopping route). The pages sit on it: the shopping list here,
//! the equipment of the ships (P6) later.
//!
//! ⚠ The data comes through [`Sources`]: the shops, the prices of the raw
//! materials, the recipes, the price of a ship and the slots of a ship. This
//! file knows none of the stores, so it can be checked with made-up ones
//! (`tools/parity_cart.py`).
//!
//! ⚠⚠ Open for the exchange contract of the Basetool (#2821): several named
//! equipments per ship (one active), the slot as `portPath` (with `/`, case
//! not significant), per slot the item, how it was got (`purchased`,
//! `crafted`, `pledge`, `looted`, `unknown`), the blueprint and the quality
//! 0 to 1000 per recipe slot (missing = 500). What is stored now is one
//! `belegung` per ship, path to `{ref, name, weg, erledigt}`; a [`Line`] knows
//! its path, its identifier and its way only, so the contract can be put on
//! top (the active equipment is read as the `belegung`, `acquisition` and
//! `quality` become fields of the part) without a change to the calculation.
//!
//! ⚠ Matching goes by the identifier (the entity of the game), never by the
//! name: `Gold` would bring `Golden Medmon`. The only way back to a recipe by
//! name is the one of the farm notes, which know the name of the blueprint
//! only.

use std::collections::{BTreeMap, BTreeSet};

use crate::hangar::{get, text, truthy};
use crate::jsonio::{Map, Value, map_set, py_eq_value};
use crate::notepad;
use crate::text::py_sum;

/// The way: buy the part.
pub const BUY: &str = "kaufen";
/// The way: build the part.
pub const CRAFT: &str = "bauen";

/// The state of a whole ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// The slots of the ship are unknown: no statement is possible.
    NoData,
    /// The equipment is the factory one: nothing to get.
    NothingOpen,
    Open,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::NoData => "keine_daten",
            State::NothingOpen => "nichts_offen",
            State::Open => "offen",
        }
    }
}

/// The state of one way at one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    Known,
    /// The part is known, UEX has no price.
    NoPrice,
    /// There is no blueprint for the part.
    NoRecipe,
    /// Nobody looked yet.
    NotChecked,
}

impl Condition {
    pub fn as_str(self) -> &'static str {
        match self {
            Condition::Known => "bekannt",
            Condition::NoPrice => "kein_preis",
            Condition::NoRecipe => "kein_rezept",
            Condition::NotChecked => "nicht_geprueft",
        }
    }
}

/// What stands on a line: a part or a whole ship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Part,
    Ship,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Part => "teil",
            Kind::Ship => "schiff",
        }
    }
}

/// Where a line comes from: the hangar (paid), the wish list or the farm
/// notes (parts without a ship).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Hangar,
    Wishlist,
    Notepad,
    /// Put in from the shops (`basket.rs`); [`invoice`] never makes such a line.
    Basket,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Hangar => "hangar",
            Origin::Wishlist => "wunsch",
            Origin::Notepad => "merkzettel",
            Origin::Basket => "korb",
        }
    }
}

/// One shop that carries something.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub shop: String,
    pub place: String,
    pub system: String,
    pub price: f64,
}

/// A slot of a ship with its factory part.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub path: String,
    /// The kind of the slot (`Cooler` …).
    pub kind: String,
    pub size: Option<i64>,
    pub factory_ref: String,
    pub factory_name: String,
}

/// One material of a recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct Ingredient {
    pub material: String,
    pub amount: f64,
}

/// A step of a recipe: how long it takes and what goes in.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub seconds: i64,
    pub ingredients: Vec<Ingredient>,
}

/// A recipe as the bill needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// The name of the blueprint the recipe was found under.
    pub blueprint: String,
    pub steps: Vec<Step>,
}

/// Where the data comes from.
pub trait Sources {
    /// Is something stored for this identifier (an empty result too)?
    fn shop_known(&self, reference: &str) -> bool;
    /// The cheapest shop: `(price, shop, place)`.
    fn cheapest(&self, reference: &str) -> Option<(f64, String, String)>;
    /// All shops that carry the part; empty where nobody does or nobody looked.
    fn shops_for(&self, reference: &str) -> Vec<Offer>;
    /// What a raw material costs per SCU; 0 where it cannot be bought.
    fn raw_buy_price(&self, material: &str) -> f64;
    /// The recipe of a part: by its identifier, else by the name of its
    /// blueprint; `None` where there is none or it has no steps.
    fn recipe_for(&self, reference: &str, name: &str) -> Option<Recipe>;
    /// Where the ship is for sale, the cheapest first.
    fn ship_offers(&self, name: &str) -> Vec<Offer>;
    /// The slots of a ship for the equipment chosen so far (a changed rack
    /// brings other slots); empty where the data is missing.
    fn slots(&self, ship: &Map, chosen: &Map) -> Vec<Slot>;
}

/// What a part costs bought.
#[derive(Clone, Debug, PartialEq)]
pub struct BuyOption {
    pub condition: Condition,
    pub price: Option<f64>,
    pub shop: String,
    pub place: String,
}

impl BuyOption {
    fn blank(condition: Condition) -> BuyOption {
        BuyOption { condition, price: None, shop: String::new(), place: String::new() }
    }
}

/// What a part costs built.
#[derive(Clone, Debug, PartialEq)]
pub struct CraftOption {
    pub condition: Condition,
    pub material: Option<f64>,
    pub seconds: Option<i64>,
    pub blueprint: String,
    /// The raw materials without a price: the sum is a lower bound.
    pub unpriced: Vec<String>,
}

impl CraftOption {
    fn blank() -> CraftOption {
        CraftOption {
            condition: Condition::NoRecipe,
            material: None,
            seconds: None,
            blueprint: String::new(),
            unpriced: Vec::new(),
        }
    }
}

/// One line of the bill.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub kind: Kind,
    pub ship: String,
    pub origin: Origin,
    /// Where on the ship (`Cooler S2`); empty for a ship and for the notes.
    pub position: String,
    pub name: String,
    pub reference: String,
    /// `kaufen` or `bauen`.
    pub way: String,
    pub buy: BuyOption,
    pub craft: CraftOption,
    pub done: bool,
    /// How many: a slot takes one part, the notes may plan more.
    pub count: i64,
    /// The slot; empty for a ship and for the notes.
    pub path: String,
    /// All slots of a grouped line (see [`group_items`]).
    pub paths: Vec<String>,
    pub slot_kind: String,
    pub size: Option<i64>,
    pub factory_ref: String,
    pub factory_name: String,
}

impl Line {
    fn pieces(&self) -> i64 {
        self.count.max(1)
    }
}

/// The result of [`invoice`].
#[derive(Clone, Debug, PartialEq)]
pub struct Invoice {
    pub lines: Vec<Line>,
    pub total: Total,
    /// How many ships take part.
    pub ships: usize,
    /// The ships with a plan whose slots are unknown.
    pub without_slot_data: Vec<String>,
}

// ------------------------------------------------------------ the equipment

fn loadout(entry: &Map) -> &[(String, Value)] {
    match get(entry, "belegung") {
        Some(Value::Object(m)) => m,
        _ => &[],
    }
}

fn loadout_mut(entry: &mut Map) -> Option<&mut Map> {
    match entry.iter_mut().find(|(k, _)| k == "belegung") {
        Some((_, Value::Object(m))) => Some(m),
        _ => None,
    }
}

fn slot_of<'a>(entry: &'a Map, path: &str) -> Option<&'a Map> {
    loadout(entry).iter().find(|(k, _)| k == path).and_then(|(_, v)| v.as_object())
}

fn slot_mut<'a>(entry: &'a mut Map, path: &str) -> Option<&'a mut Map> {
    match loadout_mut(entry)?.iter_mut().find(|(k, _)| k == path) {
        Some((_, Value::Object(slot))) => Some(slot),
        _ => None,
    }
}

/// Is this part ticked off?
pub fn is_done(entry: &Map, path: &str) -> bool {
    slot_of(entry, path).is_some_and(|slot| truthy(get(slot, "erledigt")))
}

/// Puts a part into a slot; true when something changed. The identifier is the
/// content, the name only the label.
pub fn set_part(entry: &mut Map, path: &str, reference: &str, name: &str, way: &str) -> bool {
    if path.is_empty() || reference.is_empty() {
        return false;
    }
    let new = vec![
        ("ref".to_string(), Value::str(reference)),
        ("name".to_string(), Value::str(name)),
        ("weg".to_string(), Value::str(way)),
    ];
    let old = slot_of(entry, path);
    if old.is_some_and(|old| py_eq_value(&Value::Object(old.clone()), &Value::Object(new.clone()))) {
        return false;
    }
    let other_part = old.is_none_or(|old| text(old, "ref") != reference);
    if !matches!(get(entry, "belegung"), Some(Value::Object(_))) {
        map_set(entry, "belegung", Value::Object(Map::new()));
    }
    let Some(plan) = loadout_mut(entry) else { return false };
    if other_part {
        clear_below(plan, path);
    }
    map_set(plan, path, Value::Object(new));
    true
}

/// Puts a slot back to the factory part; true when something changed.
pub fn clear_part(entry: &mut Map, path: &str) -> bool {
    let Some(plan) = loadout_mut(entry) else { return false };
    let value = plan.iter().position(|(k, _)| k == path).map(|i| plan.remove(i).1);
    let removed = value.is_some_and(|v| !matches!(v, Value::Null));
    clear_below(plan, path) || removed
}

/// Drops the choice in the slots below a slot.
///
/// ⚠⚠ Whoever swaps a rack gets other slots below it, and they are often
/// named alike (`missile_01_attach` on the torpedo rack and on the bomb rack).
/// If the old choice stayed, a torpedo would lie in a bomb slot afterwards and
/// the cart would want to buy it.
fn clear_below(plan: &mut Map, path: &str) -> bool {
    let prefix = format!("{path}/");
    let before = plan.len();
    plan.retain(|(k, _)| !k.starts_with(&prefix));
    plan.len() != before
}

/// Buy or build: the choice at a slot. It lives in the equipment, not in a
/// second field beside it.
pub fn set_method(entry: &mut Map, path: &str, way: &str) -> bool {
    if way != BUY && way != CRAFT {
        return false;
    }
    let Some(slot) = slot_mut(entry, path) else { return false };
    if slot.is_empty() || text(slot, "weg") == way {
        return false;
    }
    map_set(slot, "weg", Value::str(way));
    true
}

/// Ticks a slot off or takes the tick back; true when something changed.
///
/// ⚠⚠ Nothing is guessed: the game does not write what sits in a ship, so the
/// tool cannot notice that a bought or built part is fitted. It is ticked
/// off, as on any shopping note, for both ways alike.
pub fn set_done(entry: &mut Map, path: &str, on: bool) -> bool {
    let Some(slot) = slot_mut(entry, path) else { return false };
    if slot.is_empty() || truthy(get(slot, "erledigt")) == on {
        return false;
    }
    if on {
        map_set(slot, "erledigt", Value::Bool(true));
    } else {
        slot.retain(|(k, _)| k != "erledigt");
    }
    true
}

/// Ticks the slots of one ship off, or takes the ticks back; true when
/// something changed. The ship is searched by name **and** origin: one ship
/// can stand in the hangar and on the wish list at once (a second copy), and
/// the tick belongs to exactly one of them.
pub fn tick(data: &mut Map, origin: Origin, ship: &str, paths: &[String], on: bool) -> bool {
    let key = if origin == Origin::Wishlist { "wunsch" } else { "schiffe" };
    let Some((_, Value::Array(list))) = data.iter_mut().find(|(k, _)| k == key) else { return false };
    for item in list {
        if let Value::Object(entry) = item
            && text(entry, "name") == ship
        {
            let mut changed = false;
            for path in paths {
                changed |= set_done(entry, path, on);
            }
            return changed;
        }
    }
    false
}

// ------------------------------------------------------------ the lines

fn as_pieces(count: i64) -> i64 {
    count.max(1)
}

/// Everything on a ship that is not factory fitted.
///
/// ⚠ A slot that is empty at the factory counts, and a slot where the player
/// puts exactly the factory part does not (compared by the identifier). A
/// ticked-off line stays in the list, it is only not counted any more: whoever
/// ticked by mistake must find it again.
pub fn line_items(entry: &Map, sources: &dyn Sources) -> (State, Vec<Line>) {
    let chosen: Map = loadout(entry).to_vec();
    let slots = sources.slots(entry, &chosen);
    if slots.is_empty() {
        // ⚠ Not "nothing to get": without the slots no statement is possible.
        return (State::NoData, Vec::new());
    }
    let mut result = Vec::new();
    for (path, part) in &chosen {
        let Some(slot) = slots.iter().rev().find(|s| &s.path == path) else { continue };
        let Some(part) = part.as_object() else { continue };
        let reference = text(part, "ref");
        if reference.is_empty() || slot.factory_ref == reference {
            continue;
        }
        let way = text(part, "weg");
        result.push(Line {
            kind: Kind::Part,
            ship: String::new(),
            origin: Origin::Hangar,
            position: String::new(),
            name: text(part, "name").to_string(),
            reference: reference.to_string(),
            way: if way.is_empty() { BUY.to_string() } else { way.to_string() },
            buy: BuyOption::blank(Condition::NotChecked),
            craft: CraftOption::blank(),
            done: truthy(get(part, "erledigt")),
            count: 1,
            path: path.clone(),
            paths: Vec::new(),
            slot_kind: slot.kind.clone(),
            size: slot.size,
            factory_ref: slot.factory_ref.clone(),
            factory_name: slot.factory_name.clone(),
        });
    }
    result.sort_by(|a, b| (&a.slot_kind, &a.path).cmp(&(&b.slot_kind, &b.path)));
    let state = if result.is_empty() { State::NothingOpen } else { State::Open };
    (state, result)
}

// ------------------------------------------------------------ the two ways

/// What the part costs bought.
///
/// ⚠⚠ Three states and none a claim about the game: "not checked" says
/// nobody looked, "no price" that UEX does not carry it. UEX has gaps; a
/// "nowhere in trade" would be a statement about foreign data.
pub fn buy_option(reference: &str, sources: &dyn Sources) -> BuyOption {
    if reference.is_empty() || !sources.shop_known(reference) {
        return BuyOption::blank(Condition::NotChecked);
    }
    match sources.cheapest(reference) {
        None => BuyOption::blank(Condition::NoPrice),
        Some((price, shop, place)) => BuyOption { condition: Condition::Known, price: Some(price), shop, place },
    }
}

/// What the part costs in material when it is built.
///
/// ⚠⚠ `unpriced` is no side issue: a raw material with the buying price 0 is
/// not free but cannot be bought, it has to be mined. The sum is then a lower
/// bound, and whoever does not say so makes building look cheaper than it is.
pub fn craft_option(reference: &str, name: &str, sources: &dyn Sources) -> CraftOption {
    if reference.is_empty() && name.is_empty() {
        return CraftOption::blank();
    }
    let Some(recipe) = sources.recipe_for(reference, name) else { return CraftOption::blank() };
    if recipe.steps.is_empty() {
        return CraftOption::blank();
    }
    let mut material = 0.0_f64;
    let mut unpriced: Vec<String> = Vec::new();
    let mut seconds = 0_i64;
    for step in &recipe.steps {
        seconds += step.seconds;
        for ingredient in &step.ingredients {
            let buy = sources.raw_buy_price(&ingredient.material);
            if buy == 0.0 {
                if !unpriced.contains(&ingredient.material) {
                    unpriced.push(ingredient.material.clone());
                }
                continue;
            }
            material += buy * ingredient.amount;
        }
    }
    CraftOption {
        condition: Condition::Known,
        material: Some(material),
        seconds: Some(seconds),
        blueprint: recipe.blueprint,
        unpriced,
    }
}

/// Both ways for every line. A line without a blueprint cannot be built, the
/// way is then "buy" whatever was stored: the total must not count a way that
/// does not exist.
pub fn enrich(lines: &mut [Line], sources: &dyn Sources) {
    for line in lines {
        line.buy = buy_option(&line.reference, sources);
        line.craft = craft_option(&line.reference, "", sources);
        if line.way == CRAFT && line.craft.condition != Condition::Known {
            line.way = BUY.to_string();
        }
    }
}

// ------------------------------------------------------------ the total

/// What the cart costs.
#[derive(Clone, Debug, PartialEq)]
pub struct Total {
    pub total: f64,
    pub buy: f64,
    pub craft: f64,
    pub seconds: i64,
    /// Lines without a number: they are missing in the sum.
    pub open: usize,
    /// A built line has raw materials without a price.
    pub incomplete: bool,
}

/// What the cart costs at best, for the way chosen.
///
/// ⚠⚠ The best price, not the travel price: here counts the cheapest shop in
/// the whole verse per line, [`route`] counts the shops that lie on a short
/// route. The two differ when the cheapest shop stands where nothing else is.
/// Ticked-off lines cost nothing and are no missing price either.
pub fn total(lines: &[Line]) -> Total {
    let (mut overall, mut buy_part, mut craft_part) = (0.0_f64, 0.0_f64, 0.0_f64);
    let (mut seconds, mut open, mut incomplete) = (0_i64, 0_usize, false);
    for line in lines {
        if line.done {
            continue;
        }
        let pieces = line.pieces();
        if line.way == CRAFT {
            let Some(material) = line.craft.material.filter(|_| line.craft.condition == Condition::Known) else {
                open += 1;
                continue;
            };
            craft_part += material * pieces as f64;
            overall += material * pieces as f64;
            seconds += line.craft.seconds.unwrap_or(0) * pieces;
            if !line.craft.unpriced.is_empty() {
                incomplete = true;
            }
        } else {
            let Some(price) = line.buy.price.filter(|_| line.buy.condition == Condition::Known) else {
                open += 1;
                continue;
            };
            buy_part += price * pieces as f64;
            overall += price * pieces as f64;
        }
    }
    Total { total: overall, buy: buy_part, craft: craft_part, seconds, open, incomplete }
}

// ------------------------------------------------------------ the route

type Key = (String, String, String);

/// A place that could be the next stop: (minus the cover, the cost, the place, the slots it covers).
type Candidate = (i64, f64, (String, String), BTreeSet<Key>);

fn route_key(line: &Line) -> Key {
    (line.ship.clone(), line.origin.as_str().to_string(), line.path.clone())
}

/// The lines still to buy: with a slot, not ticked off.
fn to_buy(lines: &[Line]) -> Vec<&Line> {
    lines.iter().filter(|l| !l.path.is_empty() && !l.done && l.way == BUY).collect()
}

fn offers_of(lines: &[Line], sources: &dyn Sources) -> BTreeMap<Key, Vec<Offer>> {
    let mut result = BTreeMap::new();
    for line in to_buy(lines) {
        if line.reference.is_empty() {
            continue;
        }
        let rows = sources.shops_for(&line.reference);
        if rows.is_empty() {
            continue;
        }
        result.insert(route_key(line), rows);
    }
    result
}

/// One item at one stop.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteEntry {
    pub path: String,
    pub ship: String,
    pub name: String,
    pub price: f64,
    pub count: i64,
    pub shop: String,
}

/// A stop of the route: one place, perhaps with several shops.
#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    pub system: String,
    pub place: String,
    pub shops: Vec<String>,
    pub entries: Vec<RouteEntry>,
    pub sum: f64,
}

/// The shopping route: as few stops as possible for everything bought.
///
/// Returns the stops and the slots of the lines no shop carries.
///
/// ⚠⚠ Chosen by cover, not by price: whoever takes the cheapest shop per item
/// gets eight items in eight systems, the best on paper and a lost evening in
/// practice. The price decides only at the same cover. A stop is a place, a
/// shop a terminal in it: two shops at one station are one stop.
pub fn route(lines: &[Line], sources: &dyn Sources) -> (Vec<Stop>, Vec<String>) {
    let offers = offers_of(lines, sources);
    let buyable = to_buy(lines);
    let mut by_path: BTreeMap<Key, &Line> = BTreeMap::new();
    for line in &buyable {
        by_path.insert(route_key(line), line);
    }
    let mut open_paths: BTreeSet<Key> = offers.keys().cloned().collect();
    let without: Vec<String> =
        buyable.iter().filter(|l| !offers.contains_key(&route_key(l))).map(|l| l.path.clone()).collect();

    // Place -> {slot -> cheapest row there}.
    let mut places: BTreeMap<(String, String), BTreeMap<Key, &Offer>> = BTreeMap::new();
    for (key, rows) in &offers {
        for row in rows {
            let here = places.entry((row.system.clone(), row.place.clone())).or_default();
            // The same part in several terminals of one place counts once, at the cheapest price.
            if here.get(key).is_none_or(|old| row.price < old.price) {
                here.insert(key.clone(), row);
            }
        }
    }

    let mut stops = Vec::new();
    while !open_paths.is_empty() {
        let mut best: Option<Candidate> = None;
        for (place, here) in &places {
            let covers: BTreeSet<Key> = here.keys().filter(|k| open_paths.contains(*k)).cloned().collect();
            if covers.is_empty() {
                continue;
            }
            let cost = py_sum(covers.iter().map(|k| here[k].price));
            let cover = -(covers.len() as i64);
            // Much cover first, then cheap, then by name, so that the result stays stable.
            let better = match &best {
                None => true,
                Some((c, p, k, _)) => (cover, cost, place).partial_cmp(&(*c, *p, k)) == Some(std::cmp::Ordering::Less),
            };
            if better {
                best = Some((cover, cost, place.clone(), covers));
            }
        }
        let Some((_, _, place, covers)) = best else { break };
        let here = &places[&place];
        let mut entries = Vec::new();
        for key in &covers {
            let row = here[key];
            let count = by_path.get(key).map_or(1, |l| as_pieces(l.count));
            entries.push(RouteEntry {
                path: key.2.clone(),
                ship: key.0.clone(),
                name: by_path.get(key).map(|l| l.name.clone()).unwrap_or_default(),
                price: row.price,
                count,
                shop: row.shop.clone(),
            });
        }
        let shops: BTreeSet<String> = entries.iter().filter(|e| !e.shop.is_empty()).map(|e| e.shop.clone()).collect();
        let sum = py_sum(entries.iter().map(|e| e.price * e.count as f64));
        stops.push(Stop { system: place.0, place: place.1, shops: shops.into_iter().collect(), entries, sum });
        for key in &covers {
            open_paths.remove(key);
        }
    }
    (stops, without)
}

/// The identifier of a stop for the saved order: `System|Place`.
pub fn stop_key(stop: &Stop) -> String {
    format!("{}|{}", stop.system, stop.place)
}

/// The stops in the saved order: the ones in `order` first in that sequence,
/// new ones after them in the sequence of [`route`].
pub fn ordered_stops(stops: &[Stop], order: &[String]) -> Vec<Stop> {
    // As a dictionary of 4.12 would hold it: a key written twice counts with its last place, and what
    // is not there ranks behind the number of different keys.
    let distinct = order.iter().collect::<BTreeSet<_>>().len();
    let rank = |stop: &Stop| {
        let key = stop_key(stop);
        order.iter().rposition(|k| *k == key).unwrap_or(distinct)
    };
    let mut indexed: Vec<(usize, &Stop)> = stops.iter().enumerate().collect();
    indexed.sort_by_key(|(i, stop)| (rank(stop), *i));
    indexed.into_iter().map(|(_, stop)| stop.clone()).collect()
}

/// Moves a stop. Returns the new list and the order to save: the whole shown
/// sequence, then the stops of an earlier route that do not occur now. `None`
/// where `from` is no stop.
pub fn move_stop(stops: &[Stop], from: usize, to: usize, saved: &[String]) -> (Vec<Stop>, Option<Vec<String>>) {
    let mut stops = stops.to_vec();
    if from >= stops.len() {
        return (stops, None);
    }
    let to = to.min(stops.len() - 1);
    let moved = stops.remove(from);
    stops.insert(to, moved);
    let shown: Vec<String> = stops.iter().map(stop_key).collect();
    let rest: Vec<String> = saved.iter().filter(|k| !shown.contains(k)).cloned().collect();
    let order = shown.into_iter().chain(rest).collect();
    (stops, Some(order))
}

/// One line of a stop: the same name in the same shop at the same unit price
/// put together.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteLine {
    pub name: String,
    pub shop: String,
    /// The unit price.
    pub price: f64,
    pub count: i64,
    pub sum: f64,
}

pub fn route_lines(entries: &[RouteEntry]) -> Vec<RouteLine> {
    let mut groups: Vec<RouteLine> = Vec::new();
    for entry in entries {
        let pieces = as_pieces(entry.count);
        let found = groups
            .iter()
            .position(|g| g.name == entry.name && g.shop == entry.shop && g.price.to_bits() == entry.price.to_bits());
        let group = match found {
            Some(i) => &mut groups[i],
            None => {
                groups.push(RouteLine {
                    name: entry.name.clone(),
                    shop: entry.shop.clone(),
                    price: entry.price,
                    count: 0,
                    sum: 0.0,
                });
                groups.last_mut().expect("just pushed")
            }
        };
        group.count += pieces;
        group.sum += entry.price * pieces as f64;
    }
    groups.sort_by_cached_key(|g| (g.shop.to_lowercase(), g.name.to_lowercase()));
    groups
}

/// What the route costs and how far it leads.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteTotal {
    pub total: f64,
    pub stops: usize,
    pub shops: usize,
    pub systems: usize,
}

/// ⚠ This number belongs to the route, not the one of [`total`]: it is
/// mostly a bit higher, because not everything lies at the best price in one place.
pub fn route_total(stops: &[Stop]) -> RouteTotal {
    let systems: BTreeSet<&str> = stops.iter().map(|s| s.system.as_str()).filter(|s| !s.is_empty()).collect();
    RouteTotal {
        total: py_sum(stops.iter().map(|s| s.sum)),
        stops: stops.len(),
        shops: stops.iter().map(|s| s.shops.len()).sum(),
        systems: systems.len(),
    }
}

/// Equal lines of one ship put into one: the same ship, the same part instead
/// of the same factory part, the same way, the same tick. A copy of the first
/// with the count and all slots, in the sequence of the first occurrence. A
/// line without a slot stands for itself.
pub fn group_items(lines: &[Line]) -> Vec<Line> {
    type GroupKey = (String, Origin, Kind, String, String, String, bool, Option<usize>);
    let mut groups: Vec<(GroupKey, Line)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let key: GroupKey = (
            line.ship.clone(),
            line.origin,
            line.kind,
            if line.reference.is_empty() { line.name.clone() } else { line.reference.clone() },
            line.factory_ref.clone(),
            line.way.clone(),
            line.done,
            line.path.is_empty().then_some(index),
        );
        let at = match groups.iter().position(|(k, _)| *k == key) {
            Some(at) => at,
            None => {
                groups.push((key, Line { count: 0, paths: Vec::new(), ..line.clone() }));
                groups.len() - 1
            }
        };
        let group = &mut groups[at].1;
        group.count += as_pieces(line.count);
        if !line.path.is_empty() {
            group.paths.push(line.path.clone());
        }
    }
    groups.into_iter().map(|(_, line)| line).collect()
}

/// The lines to buy that no shop carries, equal ones put together:
/// `(name, count)` by name.
pub fn without_lines(lines: &[Line], sources: &dyn Sources) -> Vec<(String, i64)> {
    let offers = offers_of(lines, sources);
    let mut count: Vec<(String, i64)> = Vec::new();
    for line in to_buy(lines) {
        if offers.contains_key(&route_key(line)) {
            continue;
        }
        match count.iter_mut().find(|(name, _)| *name == line.name) {
            Some((_, n)) => *n += 1,
            None => count.push((line.name.clone(), 1)),
        }
    }
    count.sort_by_cached_key(|(name, _)| name.to_lowercase());
    count
}

// ------------------------------------------------------------ the bill

fn entries_of<'a>(data: &'a Map, key: &str) -> Vec<&'a Map> {
    match get(data, key) {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    }
}

fn ship_line(name: &str, sources: &dyn Sources) -> Line {
    let buy = match sources.ship_offers(name).first() {
        Some(best) => BuyOption {
            condition: Condition::Known,
            price: Some(best.price),
            shop: best.shop.clone(),
            place: best.place.clone(),
        },
        // ⚠ Mostly "not for sale for aUEC in the game" (concept ship). Not
        // claimed, only no price shown.
        None => BuyOption::blank(Condition::NoPrice),
    };
    Line {
        kind: Kind::Ship,
        ship: name.to_string(),
        origin: Origin::Wishlist,
        position: String::new(),
        name: name.to_string(),
        reference: String::new(),
        way: BUY.to_string(),
        buy,
        craft: CraftOption::blank(),
        done: false,
        count: 1,
        path: String::new(),
        paths: Vec::new(),
        slot_kind: String::new(),
        size: None,
        factory_ref: String::new(),
        factory_name: String::new(),
    }
}

/// The bill over all ships, the wish list and the farm notes.
///
/// ⚠⚠ A ship is a line itself, but only where one does not have it yet: what
/// stands in the hangar is paid, a wish ship costs itself first and its
/// equipment after it. Every line carries its position: a bill without
/// positions is a number nobody can use. No net access here: what is still to
/// look up says [`missing_prices`].
pub fn invoice(data: &Map, sources: &dyn Sources) -> Invoice {
    let mut result: Vec<Line> = Vec::new();
    let mut without_data: Vec<String> = Vec::new();
    let mut involved: BTreeSet<String> = BTreeSet::new();
    let ships = entries_of(data, "schiffe").into_iter().map(|e| (e, Origin::Hangar));
    let wishes = entries_of(data, "wunsch").into_iter().map(|e| (e, Origin::Wishlist));
    for (entry, origin) in ships.chain(wishes) {
        let name = text(entry, "name");
        if name.is_empty() {
            continue;
        }
        if origin == Origin::Wishlist {
            involved.insert(name.to_string());
            result.push(ship_line(name, sources));
        }
        let (state, parts) = line_items(entry, sources);
        if state == State::NoData {
            // Only noted where somebody has a plan; else forty concept ships would stand as a lack.
            if !loadout(entry).is_empty() {
                without_data.push(name.to_string());
            }
            continue;
        }
        for mut part in parts {
            involved.insert(name.to_string());
            part.buy = buy_option(&part.reference, sources);
            part.craft = craft_option(&part.reference, "", sources);
            if part.way == CRAFT && part.craft.condition != Condition::Known {
                part.way = BUY.to_string();
            }
            part.position = match part.size {
                Some(size) => format!("{} S{size}", part.slot_kind),
                None => part.slot_kind.clone(),
            };
            part.ship = name.to_string();
            part.origin = origin;
            result.push(part);
        }
    }
    // The notes: single parts that belong to no ship. Helmet, weapon, armour
    // are blueprints like any other, only with no slot to hang on.
    for note in notepad::notes(data) {
        if note.name.is_empty() {
            continue;
        }
        // ⚠ With the name: a note often has no identifier, it comes from the
        // crafting list, which knows the name of the blueprint only.
        let mut line = Line {
            kind: Kind::Part,
            ship: String::new(),
            origin: Origin::Notepad,
            position: String::new(),
            name: note.name.clone(),
            reference: note.reference.clone(),
            way: if note.way.is_empty() { CRAFT.to_string() } else { note.way.clone() },
            buy: buy_option(&note.reference, sources),
            craft: craft_option(&note.reference, &note.name, sources),
            done: note.done,
            count: as_pieces(note.count),
            path: String::new(),
            paths: Vec::new(),
            slot_kind: String::new(),
            size: None,
            factory_ref: String::new(),
            factory_name: String::new(),
        };
        if line.way == CRAFT && line.craft.condition != Condition::Known {
            line.way = BUY.to_string();
        }
        result.push(line);
    }
    // Ships first, then their parts, as on a bill where the main item stands
    // above the accessories. Notes have no ship and land on top by themselves.
    result.sort_by_cached_key(|l| {
        (l.ship.to_lowercase(), if l.kind == Kind::Ship { 0 } else { 1 }, l.position.clone(), l.name.to_lowercase())
    });
    let total = total(&result);
    without_data.sort();
    without_data.dedup();
    Invoice { lines: result, total, ships: involved.len(), without_slot_data: without_data }
}

/// The lines of a bill as the window shows them: by ship, the ship above its parts, then by the kind of slot,
/// within a kind by size (`size_order`: S2 before S10, no size last), then by name. [`invoice`] keeps the
/// order of 4.12, where the size stood in the text of the position.
pub fn bill_order(lines: &mut [Line]) {
    lines.sort_by_cached_key(|l| {
        let (no_size, size, name) = crate::size_order::key(l.size, &l.name);
        (l.ship.to_lowercase(), l.kind != Kind::Ship, l.slot_kind.clone(), no_size, size, name)
    });
}

/// For which lines the shop price is still to be looked up: pairs
/// `(identifier, name)`. Without this step a bill stays on "not checked" and
/// shows lines without a price although UEX knows them. The lookup itself
/// belongs to the page, where it runs in the background.
pub fn missing_prices(lines: &[Line]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for line in lines {
        if line.kind == Kind::Ship {
            // Ship prices come from the ship list, not from the shops.
            continue;
        }
        if !line.reference.is_empty() && line.buy.condition == Condition::NotChecked && seen.insert(&line.reference) {
            result.push((line.reference.clone(), line.name.clone()));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::jsonio::loads;

    #[derive(Default)]
    struct Fake {
        shops: HashMap<String, Vec<Offer>>,
        raw: HashMap<String, f64>,
        recipes: HashMap<String, Recipe>,
        ships: HashMap<String, Vec<Offer>>,
        slots: HashMap<String, Vec<Slot>>,
    }

    impl Sources for Fake {
        fn shop_known(&self, reference: &str) -> bool {
            self.shops.contains_key(reference)
        }
        fn cheapest(&self, reference: &str) -> Option<(f64, String, String)> {
            let mut best: Option<&Offer> = None;
            for row in self.shops.get(reference)? {
                if best.is_none_or(|b| row.price < b.price) {
                    best = Some(row);
                }
            }
            best.map(|o| (o.price, if o.shop.is_empty() { "?".into() } else { o.shop.clone() }, o.place.clone()))
        }
        fn shops_for(&self, reference: &str) -> Vec<Offer> {
            self.shops.get(reference).cloned().unwrap_or_default()
        }
        fn raw_buy_price(&self, material: &str) -> f64 {
            self.raw.get(material).copied().unwrap_or(0.0)
        }
        fn recipe_for(&self, reference: &str, name: &str) -> Option<Recipe> {
            self.recipes.get(reference).or_else(|| self.recipes.get(name)).cloned()
        }
        fn ship_offers(&self, name: &str) -> Vec<Offer> {
            self.ships.get(name).cloned().unwrap_or_default()
        }
        fn slots(&self, ship: &Map, _chosen: &Map) -> Vec<Slot> {
            self.slots.get(text(ship, "name")).cloned().unwrap_or_default()
        }
    }

    fn offer(shop: &str, place: &str, system: &str, price: f64) -> Offer {
        Offer { shop: shop.into(), place: place.into(), system: system.into(), price }
    }

    fn slot(path: &str, kind: &str, size: i64, factory: &str) -> Slot {
        Slot {
            path: path.into(),
            kind: kind.into(),
            size: Some(size),
            factory_ref: factory.into(),
            factory_name: format!("Factory {factory}"),
        }
    }

    fn recipe(blueprint: &str, seconds: i64, ingredients: &[(&str, f64)]) -> Recipe {
        Recipe {
            blueprint: blueprint.into(),
            steps: vec![Step {
                seconds,
                ingredients: ingredients
                    .iter()
                    .map(|(m, a)| Ingredient { material: (*m).into(), amount: *a })
                    .collect(),
            }],
        }
    }

    fn data(json: &str) -> Map {
        match loads(json).unwrap() {
            Value::Object(m) => m,
            other => panic!("{other:?}"),
        }
    }

    fn part(ship: &str, path: &str, reference: &str, name: &str, way: &str) -> Line {
        Line {
            kind: Kind::Part,
            ship: ship.into(),
            origin: Origin::Hangar,
            position: String::new(),
            name: name.into(),
            reference: reference.into(),
            way: way.into(),
            buy: BuyOption::blank(Condition::NotChecked),
            craft: CraftOption::blank(),
            done: false,
            count: 1,
            path: path.into(),
            paths: Vec::new(),
            slot_kind: String::new(),
            size: None,
            factory_ref: String::new(),
            factory_name: String::new(),
        }
    }

    #[test]
    fn buying_knows_three_states_and_none_is_a_claim_about_the_game() {
        let mut fake = Fake::default();
        fake.shops.insert(
            "known".into(),
            vec![offer("Dumper", "Area18", "Stanton", 22700.0), offer("", "Lorville", "Stanton", 12500.0)],
        );
        fake.shops.insert("empty".into(), Vec::new());
        let known = buy_option("known", &fake);
        assert_eq!((known.condition, known.price), (Condition::Known, Some(12500.0)));
        assert_eq!((known.shop.as_str(), known.place.as_str()), ("?", "Lorville"));
        assert_eq!(buy_option("empty", &fake).condition, Condition::NoPrice);
        assert_eq!(buy_option("never-looked", &fake).condition, Condition::NotChecked);
        assert_eq!(buy_option("", &fake).condition, Condition::NotChecked);
    }

    #[test]
    fn building_adds_the_material_and_names_what_has_no_price() {
        let mut fake = Fake::default();
        fake.raw.insert("Iron".into(), 10.0);
        fake.raw.insert("Quantainium".into(), 0.0);
        fake.recipes.insert(
            "cooler".into(),
            recipe("Cooler", 260, &[("Iron", 3.0), ("Quantainium", 2.0), ("Quantainium", 1.0)]),
        );
        let option = craft_option("cooler", "", &fake);
        assert_eq!(option.condition, Condition::Known);
        assert_eq!((option.material, option.seconds), (Some(30.0), Some(260)));
        assert_eq!(option.unpriced, vec!["Quantainium".to_string()], "named once, not counted as free");
        // The way back by the name of the blueprint, for a note without an identifier.
        assert_eq!(craft_option("", "cooler", &fake).condition, Condition::Known);
        assert_eq!(craft_option("other", "", &fake).condition, Condition::NoRecipe);
        assert_eq!(craft_option("", "", &fake).condition, Condition::NoRecipe);
        // A recipe without a step is none.
        fake.recipes.insert("empty".into(), Recipe { blueprint: "Empty".into(), steps: Vec::new() });
        assert_eq!(craft_option("empty", "", &fake).condition, Condition::NoRecipe);
    }

    fn priced(way: &str, buy: Option<f64>, craft: Option<(f64, i64, bool)>, count: i64, done: bool) -> Line {
        let mut line = part("S", "p", "r", "n", way);
        line.count = count;
        line.done = done;
        if let Some(price) = buy {
            line.buy =
                BuyOption { condition: Condition::Known, price: Some(price), shop: "x".into(), place: "y".into() };
        }
        if let Some((material, seconds, incomplete)) = craft {
            line.craft = CraftOption {
                condition: Condition::Known,
                material: Some(material),
                seconds: Some(seconds),
                blueprint: "b".into(),
                unpriced: if incomplete { vec!["Q".into()] } else { Vec::new() },
            };
        }
        line
    }

    #[test]
    fn the_total_follows_the_way_chosen_and_counts_the_pieces() {
        let lines = vec![
            priced(BUY, Some(100.0), None, 1, false),
            priced(CRAFT, Some(500.0), Some((40.0, 60, true)), 3, false),
            priced(BUY, Some(9999.0), None, 1, true),
            priced(BUY, None, None, 1, false),
            priced(CRAFT, None, None, 1, false),
        ];
        let total = total(&lines);
        assert_eq!((total.total, total.buy, total.craft), (220.0, 100.0, 120.0));
        assert_eq!(total.seconds, 180);
        assert_eq!(total.open, 2, "a line without a number is counted, a ticked one is not");
        assert!(total.incomplete);
    }

    fn route_fake() -> (Fake, Vec<Line>) {
        let mut fake = Fake::default();
        fake.shops.insert("r1".into(), vec![offer("A1", "PX", "Stanton", 100.0), offer("B1", "PY", "Stanton", 90.0)]);
        fake.shops.insert(
            "r2".into(),
            vec![
                offer("A2", "PX", "Stanton", 200.0),
                offer("A2b", "PX", "Stanton", 180.0),
                offer("C2", "PZ", "Pyro", 150.0),
            ],
        );
        fake.shops.insert("r3".into(), vec![offer("C3", "PZ", "Pyro", 300.0)]);
        fake.shops.insert("r4".into(), Vec::new());
        let mut lines = vec![
            part("Cutlass", "p1", "r1", "One", BUY),
            part("Cutlass", "p2", "r2", "Two", BUY),
            part("Cutlass", "p3", "r3", "Three", BUY),
            part("Cutlass", "p4", "r4", "Four", BUY),
            part("Cutlass", "p5", "r1", "Ticked", BUY),
            part("Cutlass", "p6", "r1", "Built", CRAFT),
        ];
        lines[4].done = true;
        (fake, lines)
    }

    #[test]
    fn the_route_takes_the_cover_before_the_price() {
        let (fake, lines) = route_fake();
        let (stops, without) = route(&lines, &fake);
        // PX covers two (280 aUEC), PZ covers two (450), PY one: PX first, then PZ for what is left.
        assert_eq!(stops.iter().map(|s| s.place.as_str()).collect::<Vec<_>>(), ["PX", "PZ"]);
        assert_eq!(
            stops[0].entries.iter().map(|e| (e.name.as_str(), e.price)).collect::<Vec<_>>(),
            [("One", 100.0), ("Two", 180.0)]
        );
        assert_eq!(
            stops[0].shops,
            vec!["A1".to_string(), "A2b".to_string()],
            "two terminals of one place are one stop"
        );
        assert_eq!(stops[1].entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Three"]);
        assert_eq!(
            without,
            vec!["p4".to_string()],
            "a shop-less line is named, a ticked and a built one are no business of the route"
        );
        let sum = route_total(&stops);
        assert_eq!((sum.total, sum.stops, sum.shops, sum.systems), (580.0, 2, 3, 2));
        assert_eq!(without_lines(&lines, &fake), vec![("Four".to_string(), 1)]);
    }

    #[test]
    fn at_the_same_cover_the_cheaper_place_wins() {
        let mut fake = Fake::default();
        fake.shops.insert(
            "a".into(),
            vec![offer("S1", "Alpha Dear", "Stanton", 500.0), offer("S2", "Zeta Cheap", "Stanton", 400.0)],
        );
        let lines = vec![part("Ship", "p", "a", "Thing", BUY)];
        let (stops, _) = route(&lines, &fake);
        // The cheaper place stands behind the dearer one by name: the price decides, not the name.
        assert_eq!(stops[0].place, "Zeta Cheap");
    }

    #[test]
    fn the_order_of_the_stops_is_saved_whole_and_the_old_ones_stay_behind() {
        let (fake, lines) = route_fake();
        let (stops, _) = route(&lines, &fake);
        let saved = vec!["Pyro|PZ".to_string(), "Nyx|Old".to_string()];
        let ordered = ordered_stops(&stops, &saved);
        assert_eq!(ordered.iter().map(|s| s.place.as_str()).collect::<Vec<_>>(), ["PZ", "PX"]);
        let (moved, order) = move_stop(&ordered, 0, 5, &saved);
        assert_eq!(moved.iter().map(|s| s.place.as_str()).collect::<Vec<_>>(), ["PX", "PZ"]);
        assert_eq!(order.unwrap(), vec!["Stanton|PX".to_string(), "Pyro|PZ".to_string(), "Nyx|Old".to_string()]);
        assert_eq!(move_stop(&ordered, 7, 0, &saved).1, None);
        // A key written twice counts with its last place, as in a dictionary.
        let twice = vec!["Pyro|PZ".to_string(), "Stanton|PX".to_string(), "Pyro|PZ".to_string()];
        let ordered = ordered_stops(&stops, &twice);
        assert_eq!(ordered.iter().map(|s| s.place.as_str()).collect::<Vec<_>>(), ["PX", "PZ"]);
    }

    #[test]
    fn equal_stops_of_one_shop_are_put_together() {
        let entry = |name: &str, shop: &str, price: f64, count: i64| RouteEntry {
            path: "p".into(),
            ship: "S".into(),
            name: name.into(),
            price,
            count,
            shop: shop.into(),
        };
        let lines = route_lines(&[
            entry("Cooler", "Zed", 10.0, 1),
            entry("Cooler", "Zed", 10.0, 2),
            entry("Cooler", "Zed", 12.0, 1),
            entry("Armor", "alpha", 5.0, 1),
        ]);
        assert_eq!(
            lines.iter().map(|l| (l.name.as_str(), l.price, l.count, l.sum)).collect::<Vec<_>>(),
            [("Armor", 5.0, 1, 5.0), ("Cooler", 10.0, 3, 30.0), ("Cooler", 12.0, 1, 12.0)]
        );
    }

    #[test]
    fn grouping_puts_equal_slots_in_one_line_and_leaves_the_notes_alone() {
        let mut lines = vec![
            part("S", "a", "r", "Cooler", BUY),
            part("S", "b", "r", "Cooler", BUY),
            part("S", "c", "r", "Cooler", CRAFT),
            part("S", "d", "r", "Cooler", BUY),
        ];
        lines[3].done = true;
        let mut note_a = part("", "", "n", "Helm", CRAFT);
        note_a.origin = Origin::Notepad;
        let note_b = note_a.clone();
        lines.extend([note_a, note_b]);
        let grouped = group_items(&lines);
        assert_eq!(grouped.len(), 5, "the way and the tick separate, the notes never join");
        assert_eq!((grouped[0].count, grouped[0].paths.clone()), (2, vec!["a".to_string(), "b".to_string()]));
        assert_eq!(grouped[3].count, 1);
    }

    fn ships() -> (Fake, Map) {
        let mut fake = Fake::default();
        fake.slots.insert(
            "Cutlass".into(),
            vec![slot("cooler", "Cooler", 2, "f-old"), slot("battery", "Battery", 1, "same-ref")],
        );
        fake.slots.insert("Polaris".into(), vec![slot("cooler", "Cooler", 3, "")]);
        fake.shops.insert("c-new".into(), vec![offer("Dumper", "Area18", "Stanton", 22700.0)]);
        fake.recipes.insert("c-new".into(), recipe("New Cooler", 120, &[("Iron", 2.0)]));
        fake.recipes.insert("Helm".into(), recipe("Helm", 60, &[("Iron", 5.0)]));
        fake.raw.insert("Iron".into(), 10.0);
        fake.ships.insert("Polaris".into(), vec![offer("Pad", "Port Olisar", "Stanton", 1_000_000.0)]);
        let data = data(
            r#"{"schiffe": [{"name": "Cutlass", "belegung": {
                  "cooler": {"ref": "c-new", "name": "New Cooler", "weg": "bauen"},
                  "battery": {"ref": "same-ref", "name": "Same", "weg": "kaufen"},
                  "gone": {"ref": "x", "name": "Slot that is gone"}}},
                 {"name": "Galaxy", "belegung": {"a": {"ref": "x", "name": "Y"}}},
                 {"name": "Idle", "belegung": {}}],
               "wunsch": [{"name": "Polaris", "belegung": {}}],
               "merkzettel": [{"name": "Helm", "ref": "", "anzahl": 2, "weg": "bauen"}]}"#,
        );
        (fake, data)
    }

    #[test]
    fn the_bill_has_the_ship_only_when_it_is_wished_and_the_notes_on_top() {
        let (fake, data) = ships();
        let bill = invoice(&data, &fake);
        let names: Vec<(&str, &str, Kind, &str)> =
            bill.lines.iter().map(|l| (l.ship.as_str(), l.name.as_str(), l.kind, l.position.as_str())).collect();
        assert_eq!(
            names,
            [
                ("", "Helm", Kind::Part, ""),
                ("Cutlass", "New Cooler", Kind::Part, "Cooler S2"),
                ("Polaris", "Polaris", Kind::Ship, ""),
            ]
        );
        assert_eq!(bill.ships, 2, "the wish ship and the ship with a part, not the idle one");
        assert_eq!(bill.without_slot_data, vec!["Galaxy".to_string()], "only where somebody has a plan");
        // The note has no identifier and is found by its name: built, twice.
        let helm = &bill.lines[0];
        assert_eq!((helm.way.as_str(), helm.count, helm.craft.material), (CRAFT, 2, Some(50.0)));
        // The part of the Cutlass is built: 2 Iron at 10.
        assert_eq!(bill.lines[1].craft.material, Some(20.0));
        // The ship costs itself.
        assert_eq!(bill.lines[2].buy.price, Some(1_000_000.0));
        assert_eq!(bill.total.total, 1_000_000.0 + 20.0 + 100.0);
        assert_eq!(bill.total.seconds, 120 + 2 * 60);
    }

    #[test]
    fn the_window_shows_the_parts_of_a_kind_by_size_and_s10_after_s2() {
        let sized = |ship: &str, kind: &str, size: Option<i64>, name: &str| Line {
            slot_kind: kind.into(),
            size,
            position: size.map_or_else(|| kind.to_string(), |s| format!("{kind} S{s}")),
            ..part(ship, "", "", name, BUY)
        };
        let mut ship = part("Polaris", "", "", "Polaris", BUY);
        ship.kind = Kind::Ship;
        let mut lines = vec![
            sized("Polaris", "WeaponGun", Some(10), "Omnisky XII Cannon"),
            sized("Polaris", "WeaponGun", None, "Aaron Mount"),
            sized("Polaris", "WeaponGun", Some(2), "Omnisky VI Cannon"),
            sized("Polaris", "Cooler", Some(3), "Arctic"),
            ship,
            sized("Polaris", "WeaponGun", Some(3), "Omnisky IX Cannon"),
            sized("Cutlass", "Cooler", Some(1), "Wave"),
        ];
        bill_order(&mut lines);
        let names: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Wave",
                "Polaris",
                "Arctic",
                "Omnisky VI Cannon",
                "Omnisky IX Cannon",
                "Omnisky XII Cannon",
                "Aaron Mount"
            ],
            "by ship, the ship first, by kind, by size (S2 before S10), no size last"
        );
    }

    #[test]
    fn a_way_without_a_recipe_falls_back_to_buying() {
        let (mut fake, data) = ships();
        fake.recipes.remove("c-new");
        let bill = invoice(&data, &fake);
        let cooler = bill.lines.iter().find(|l| l.name == "New Cooler").unwrap();
        assert_eq!((cooler.way.as_str(), cooler.craft.condition), (BUY, Condition::NoRecipe));
    }

    #[test]
    fn what_is_to_be_looked_up_is_named_once_and_not_for_ships() {
        let (fake, data) = ships();
        let mut bill = invoice(&data, &fake);
        bill.lines.push(part("Cutlass", "z", "not-looked", "Other", BUY));
        bill.lines.push(part("Cutlass", "y", "not-looked", "Other", BUY));
        let mut ship = part("Polaris", "", "ship-ref", "Polaris", BUY);
        ship.kind = Kind::Ship;
        bill.lines.push(ship);
        assert_eq!(missing_prices(&bill.lines), vec![("not-looked".to_string(), "Other".to_string())]);
    }

    #[test]
    fn the_equipment_is_edited_in_one_place_and_a_rack_swap_drops_what_hangs_below() {
        let mut entry = data(
            r#"{"name": "Eclipse", "belegung": {"rack": {"ref": "torpedo", "name": "T", "weg": "kaufen", "erledigt": true},
            "rack/missile_01": {"ref": "t1", "name": "T1", "weg": "kaufen"}, "rack2": {"ref": "r2", "name": "R2", "weg": "kaufen"},
            "other": {"ref": "o", "name": "O", "weg": "bauen"}}}"#,
        );
        assert!(is_done(&entry, "rack"));
        assert!(!set_part(&mut entry, "", "x", "X", BUY));
        assert!(!set_part(&mut entry, "rack", "", "X", BUY));
        assert!(set_part(&mut entry, "rack", "bomb", "Bomb", BUY));
        assert!(loadout(&entry).iter().all(|(k, _)| k != "rack/missile_01"), "the choice below is gone");
        assert!(
            loadout(&entry).iter().any(|(k, _)| k == "rack2"),
            "a slot that only starts with the same letters stays"
        );
        assert!(!is_done(&entry, "rack"), "a new part is not ticked");
        assert!(!set_part(&mut entry, "rack", "bomb", "Bomb", BUY), "the same again changes nothing");
        assert!(set_method(&mut entry, "rack", CRAFT) && !set_method(&mut entry, "rack", CRAFT));
        assert!(!set_method(&mut entry, "rack", "other") && !set_method(&mut entry, "nowhere", CRAFT));
        assert!(set_done(&mut entry, "other", true) && !set_done(&mut entry, "other", true));
        assert!(set_done(&mut entry, "other", false) && !is_done(&entry, "other"));
        assert!(clear_part(&mut entry, "rack") && !clear_part(&mut entry, "rack"));
        assert_eq!(loadout(&entry).len(), 2, "the slot beside and the other one");
        // The tick goes to the ship of the origin asked for, and to every slot named.
        let mut fleet = data(
            r#"{"schiffe": [{"name": "Twin", "belegung": {"a": {"ref": "x"}, "b": {"ref": "y"}}}],
               "wunsch": [{"name": "Twin", "belegung": {"a": {"ref": "x"}}}]}"#,
        );
        let both = ["a".to_string(), "b".to_string(), "none".to_string()];
        assert!(tick(&mut fleet, Origin::Wishlist, "Twin", &both, true));
        let ticked = |fleet: &Map, list: &str| -> Vec<bool> {
            let Some(Value::Array(items)) = get(fleet, list) else { panic!("list") };
            let Some(Value::Object(ship)) = items.first() else { panic!("ship") };
            ["a", "b"].iter().map(|p| is_done(ship, p)).collect()
        };
        assert_eq!(ticked(&fleet, "wunsch"), [true, false], "only the slot that is there");
        assert_eq!(ticked(&fleet, "schiffe"), [false, false], "the hangar copy stays as it was");
        assert!(
            tick(&mut fleet, Origin::Hangar, "Twin", &both, true)
                && !tick(&mut fleet, Origin::Hangar, "Twin", &both, true)
        );
        assert_eq!(ticked(&fleet, "schiffe"), [true, true]);
        assert!(tick(&mut fleet, Origin::Hangar, "Twin", &both, false));
        assert!(!tick(&mut fleet, Origin::Hangar, "Other", &both, true));
        // A ship without an equipment gets one when a part is put in.
        let mut bare = data(r#"{"name": "Bare"}"#);
        assert!(set_part(&mut bare, "p", "r", "R", BUY) && !loadout(&bare).is_empty());
    }
}
